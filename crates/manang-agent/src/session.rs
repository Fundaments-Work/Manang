//! The orchestrator loop: model turn, tool calls, results, next turn.
//!
//! Tokio is used only to stream, to wait (timers, cancellation, approvals) and to await tool
//! results. Every tool call is CPU or kernel work: it is handed to the Rayon pool and awaited
//! through a oneshot, never run on a Tokio worker (AGENTS.md section 2.3).
//!
//! History is append-only and lossless: the blocks a provider produced are stored and replayed
//! verbatim (thinking blocks and their signatures included). The view that requests are built
//! from starts as that history and is rewritten only by compaction (`manang_agent_core::compact`),
//! which runs rarely because every rewrite breaks the provider's cached prefix.

use crate::error::AgentError;
use crate::risk::{Decision, Gate, gate};
use crate::state::{PendingApproval, ProposalState, Session, Shared, StoredProposal};
use crate::storage::now_ms;
use crate::text::{clip_chars, estimate_tokens};
use manang_agent_core::compact::{
    self, Compaction, KEEP_RESULTS, KEEP_TURNS, SUMMARY_PROMPT, SUMMARY_TOKENS,
};
use manang_agent_core::{
    AuditEntry, AuditEvent, BlockKind, CacheBreakpoint, ChatRequest, ContentBlock, Message,
    PriceTable, Provider, ProviderError, ProviderEvent, RedactionConfig, Role, StopReason,
    ToolNameMap, ToolSpec, ToolUseView, TranscriptEntry, Usage, payload_hash, redact,
};
use manang_kernel::{AgentToolError, KernelError, Payload, PluginError};
use serde_json::{Value, json};
use std::collections::HashSet;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

/// Everything a session task needs, fixed when the message is sent.
pub struct TurnPlan {
    pub provider: Arc<dyn Provider>,
    pub provider_id: String,
    pub model: String,
    pub system: String,
    pub tools: Vec<ToolSpec>,
    pub names: ToolNameMap,
    pub max_tokens: u32,
    /// Model rounds per author message.
    pub max_iterations: u32,
    /// Working window in tokens; compaction thresholds are shares of it.
    pub window: u64,
    pub include_thinking: bool,
    pub cache: bool,
    pub redaction: RedactionConfig,
    pub redaction_count: u32,
    pub prices: PriceTable,
}

impl TurnPlan {
    /// The request for the current view of the conversation.
    pub fn request(&self, view: &[Message]) -> ChatRequest {
        let mut breakpoints = Vec::new();
        if self.cache {
            if !self.tools.is_empty() {
                breakpoints.push(CacheBreakpoint::Tools);
            }
            breakpoints.push(CacheBreakpoint::System);
        }
        ChatRequest {
            model: self.model.clone(),
            system: Some(self.system.clone()),
            messages: view.to_vec(),
            tools: self.tools.clone(),
            max_tokens: self.max_tokens,
            cache_breakpoints: breakpoints,
        }
    }
}

enum End {
    Done(String),
    Cancelled,
    Failed(ProviderError),
}

fn failed(code: &str, message: impl Into<String>, retryable: bool) -> End {
    End::Failed(ProviderError {
        code: code.to_owned(),
        message: message.into(),
        retryable,
        request_id: None,
    })
}

impl From<AgentError> for End {
    fn from(error: AgentError) -> Self {
        failed(error.code, error.message, false)
    }
}

fn stop_name(stop: &StopReason) -> String {
    match stop {
        StopReason::EndTurn => "end_turn".to_owned(),
        StopReason::MaxTokens => "max_tokens".to_owned(),
        StopReason::StopSequence => "stop_sequence".to_owned(),
        StopReason::ToolUse => "tool_use".to_owned(),
        StopReason::PauseTurn => "pause_turn".to_owned(),
        StopReason::Refusal => "refusal".to_owned(),
        StopReason::Other(s) => s.clone(),
    }
}

/// What one author message cost, for the single `agent.usage` event at its end.
#[derive(Default)]
struct Stats {
    usage: Usage,
    cost_micro: Option<u64>,
    requests: u32,
    compacted: Option<&'static str>,
}

/// Runs a session until it ends, then reports how (events, meta, pending approvals).
pub async fn run(
    shared: Arc<Shared>,
    session: Arc<Session>,
    plan: TurnPlan,
    cancel: CancellationToken,
) {
    let sid = session.id.clone();
    let mut stats = Stats::default();
    let end = drive(&shared, &session, &plan, &cancel, &mut stats).await;
    // Anything still waiting for the user belongs to a session that is over.
    shared.approvals.lock().retain(|_, p| p.session_id != sid);
    // The session is idle (and its meta on disk) before the UI hears that it is done.
    {
        let mut inner = session.inner.lock();
        inner.running = false;
        inner.cancel = None;
    }
    let meta_error = crate::plugin::write_meta(&session, "idle").err();
    if stats.requests > 0 {
        emit_usage(&shared, &session, &plan, &stats);
    }
    match end {
        End::Done(reason) => shared.emit(
            "agent.done",
            &sid,
            json!({ "stop_reason": reason, "steps": plan.max_iterations }),
        ),
        End::Cancelled => shared.emit(
            "agent.done",
            &sid,
            json!({ "stop_reason": "cancelled", "cancelled": true }),
        ),
        End::Failed(e) => {
            shared.emit(
                "agent.error",
                &sid,
                json!({
                    "code": e.code,
                    "message": crate::provider::friendly(&e.code, &e.message),
                    "detail": e.message,
                    "retryable": e.retryable,
                    "request_id": e.request_id,
                }),
            );
            shared.emit("agent.done", &sid, json!({ "stop_reason": "error" }));
        }
    }
    if let Some(e) = meta_error {
        shared.emit(
            "agent.error",
            &sid,
            json!({ "code": e.code, "message": e.message, "retryable": false, "request_id": null }),
        );
    }
}

fn emit_usage(shared: &Shared, session: &Session, plan: &TurnPlan, stats: &Stats) {
    let (total, total_cost, fill) = {
        let inner = session.inner.lock();
        (inner.usage, inner.cost_micro, inner.last_fill)
    };
    let u = stats.usage;
    let prompt = u.input + u.cache_read + u.cache_write;
    shared.emit(
        "agent.usage",
        &session.id,
        json!({
            "model": plan.model,
            "input": u.input, "output": u.output,
            "cache_read": u.cache_read, "cache_write": u.cache_write,
            "est_cost_micro_usd": stats.cost_micro,
            "requests": stats.requests,
            "cache_hit_percent": (u.cache_read * 100).checked_div(prompt).unwrap_or(0),
            "fill": fill,
            "window": plan.window,
            "compacted": stats.compacted,
            "total": {
                "input": total.input, "output": total.output,
                "cache_read": total.cache_read, "cache_write": total.cache_write,
                "est_cost_micro_usd": total_cost,
            },
        }),
    );
}

/// An identical read-only call within one author message returns a short note, not the text again.
#[derive(Default)]
struct RepeatGuard {
    seen: HashSet<String>,
}

impl RepeatGuard {
    fn repeated(&mut self, call: &ToolUseView, cacheable: bool) -> bool {
        cacheable && !self.seen.insert(format!("{}|{}", call.name, call.input))
    }

    /// A proposal changes what the reading tools return, so earlier reads no longer count.
    fn reset(&mut self) {
        self.seen.clear();
    }
}

async fn drive(
    shared: &Arc<Shared>,
    session: &Arc<Session>,
    plan: &TurnPlan,
    cancel: &CancellationToken,
    stats: &mut Stats,
) -> End {
    let mut guard = RepeatGuard::default();
    for _ in 0..plan.max_iterations {
        if cancel.is_cancelled() {
            return End::Cancelled;
        }
        if let Some(end) = compact_if_needed(shared, session, plan, cancel, stats).await {
            return end;
        }
        let request = plan.request(&session.inner.lock().view);
        let turn = match stream_turn(shared, session, plan, &request, cancel).await {
            Ok(turn) => turn,
            Err(end) => return end,
        };
        if let Some(usage) = turn.usage
            && let Err(e) = record_usage(session, plan, &usage, stats)
        {
            return e.into();
        }
        if !turn.content.is_empty()
            && let Err(e) = commit(session, Message::new(Role::Assistant, turn.content.clone()))
        {
            return e.into();
        }
        if turn.calls.is_empty() {
            return End::Done(stop_name(&turn.stop));
        }
        let (results, cancelled, asked) =
            run_tools(shared, session, plan, &turn.calls, cancel, &mut guard).await;
        if let Err(e) = commit(session, Message::new(Role::User, results)) {
            return e.into();
        }
        if cancelled {
            return End::Cancelled;
        }
        if asked {
            return End::Done("ask_user".to_owned());
        }
    }
    End::Done("iteration_cap".to_owned())
}

/// Appends a message to the history, the request view and the transcript.
pub fn commit(session: &Session, message: Message) -> Result<(), AgentError> {
    let seq = {
        let mut inner = session.inner.lock();
        inner.seq += 1;
        inner.history.push(message.clone());
        inner.view.push(message.clone());
        inner.seq
    };
    session
        .store
        .append_transcript(&TranscriptEntry { seq, message })
}

fn audit(session: &Session, event: AuditEvent) -> Result<(), AgentError> {
    session.store.append_audit(&AuditEntry {
        ts_ms: now_ms(),
        session: session.id.clone(),
        event,
    })
}

fn record_usage(
    session: &Session,
    plan: &TurnPlan,
    usage: &Usage,
    stats: &mut Stats,
) -> Result<(), AgentError> {
    let cost = plan
        .prices
        .estimate(&plan.model, usage)
        .map(|c| c.micro_usd);
    {
        let mut inner = session.inner.lock();
        inner.usage = inner.usage.add(usage);
        inner.cost_micro = inner.cost_micro.saturating_add(cost.unwrap_or(0));
        inner.turns += 1;
        // What the provider counted for this prompt: the fill level of the next request.
        inner.last_fill = usage.input + usage.cache_read + usage.cache_write;
    }
    stats.usage = stats.usage.add(usage);
    stats.cost_micro = match (stats.cost_micro, cost) {
        (Some(a), Some(b)) => Some(a.saturating_add(b)),
        (None, c) => c,
        (a, None) => a,
    };
    stats.requests += 1;
    audit(
        session,
        AuditEvent::Usage {
            model: plan.model.clone(),
            usage: *usage,
            cost_micro_usd: cost,
        },
    )?;
    Ok(())
}

/// Appends the numbers of one response to the global ledger (numbers only: no text, no key).
fn ledger(shared: &Shared, session: &Session, plan: &TurnPlan, usage: &Usage) {
    let cost = plan
        .prices
        .estimate(&plan.model, usage)
        .map(|c| c.micro_usd);
    let _ = shared.global.append_usage(&json!({
        "version": crate::storage::FORMAT_VERSION,
        "ts_ms": now_ms(),
        "session": session.id,
        "provider": plan.provider_id,
        "model": plan.model,
        "input": usage.input, "output": usage.output,
        "cache_read": usage.cache_read, "cache_write": usage.cache_write,
        "est_cost_micro_usd": cost,
    }));
}

// ---- compaction ---------------------------------------------------------------------------

async fn compact_if_needed(
    shared: &Arc<Shared>,
    session: &Arc<Session>,
    plan: &TurnPlan,
    cancel: &CancellationToken,
    stats: &mut Stats,
) -> Option<End> {
    let (fill, cleared) = {
        let inner = session.inner.lock();
        (inner.last_fill, inner.cleared)
    };
    match compact::decide(fill, plan.window, cleared) {
        Compaction::None => None,
        Compaction::ClearResults => {
            clear_results(session, stats);
            None
        }
        Compaction::Summarise => summarise(shared, session, plan, cancel, stats).await,
    }
}

fn clear_results(session: &Session, stats: &mut Stats) {
    let view = session.inner.lock().view.clone();
    let (next, cleared) = compact::clear_old_results(&view, KEEP_RESULTS);
    if cleared > 0 {
        let mut inner = session.inner.lock();
        inner.view = next;
        inner.cleared = true;
    }
    if cleared > 0 {
        stats.compacted = Some("cleared");
        let _ = audit(
            session,
            AuditEvent::Note {
                text: format!("context: cleared {cleared} old tool results"),
            },
        );
    }
}

/// Stage 2: one provider call without tools; the view becomes header, summary, last two turns.
async fn summarise(
    shared: &Arc<Shared>,
    session: &Arc<Session>,
    plan: &TurnPlan,
    cancel: &CancellationToken,
    stats: &mut Stats,
) -> Option<End> {
    let (view, header) = {
        let inner = session.inner.lock();
        (inner.view.clone(), inner.header.clone())
    };
    let Some((older, recent)) = compact::split_for_summary(&view, KEEP_TURNS) else {
        // Nothing older than the last two turns: all that is left is stage 1.
        if !session.inner.lock().cleared {
            clear_results(session, stats);
        }
        session.inner.lock().last_fill = 0;
        return None;
    };
    let request = ChatRequest {
        model: plan.model.clone(),
        system: Some(SUMMARY_PROMPT.to_owned()),
        messages: vec![Message::user_text(&compact::render_for_summary(older, 60_000)).ok()?],
        tools: Vec::new(),
        max_tokens: SUMMARY_TOKENS + 300,
        cache_breakpoints: Vec::new(),
    };
    let (summary, usage) = match collect_text(shared, plan, &request, cancel).await {
        Ok(done) => done,
        Err(end) => return Some(end),
    };
    if let Some(u) = usage {
        if let Err(e) = record_usage(session, plan, &u, stats) {
            return Some(e.into());
        }
        ledger(shared, session, plan, &u);
    }
    let summary = summary.trim();
    if summary.is_empty() {
        // The model gave nothing: keep the view and do not try again at once.
        session.inner.lock().last_fill = 0;
        return None;
    }
    let next = compact::rebuild_after_summary(&header, summary, recent)?;
    let estimate = next
        .iter()
        .map(|m| estimate_tokens(&serde_json::to_string(m).unwrap_or_default()))
        .sum::<u64>()
        + estimate_tokens(&plan.system);
    {
        let mut inner = session.inner.lock();
        inner.view = next;
        inner.cleared = false;
        inner.last_fill = estimate;
    }
    stats.compacted = Some("summarised");
    let _ = audit(
        session,
        AuditEvent::Note {
            text: format!("context: summarised {} earlier messages", older.len()),
        },
    );
    if compact::decide(estimate, plan.window, true) == Compaction::Summarise {
        return Some(End::Done("context_full".to_owned()));
    }
    None
}

/// One provider call whose text is collected (no tools, no events to the UI).
async fn collect_text(
    shared: &Arc<Shared>,
    plan: &TurnPlan,
    request: &ChatRequest,
    cancel: &CancellationToken,
) -> Result<(String, Option<Usage>), End> {
    let mut stream = plan.provider.stream(request);
    let mut text = String::new();
    let mut usage = None;
    loop {
        let next = tokio::select! {
            biased;
            () = cancel.cancelled() => return Err(End::Cancelled),
            next = tokio::time::timeout(
                shared.limits.idle,
                futures_util::StreamExt::next(&mut stream),
            ) => next,
        };
        match next {
            Err(_) => return Err(failed("timeout", "the provider stopped responding", true)),
            Ok(None) => break,
            Ok(Some(ProviderEvent::TextDelta { text: t })) => text.push_str(&t),
            Ok(Some(ProviderEvent::Usage { usage: u })) => usage = Some(u),
            Ok(Some(ProviderEvent::Done { .. })) => break,
            Ok(Some(ProviderEvent::Error { error })) => return Err(End::Failed(error)),
            Ok(Some(_)) => {}
        }
    }
    Ok((text, usage))
}

// ---- one model turn -----------------------------------------------------------------------

struct Turn {
    content: Vec<ContentBlock>,
    calls: Vec<ToolUseView>,
    usage: Option<Usage>,
    stop: StopReason,
}

/// Streaming deltas are sent in batches, so a fast model does not flood the IPC. Reasoning is
/// shown only as a "thinking" signal unless the developer flag asks for its text.
struct Batch {
    text: String,
    thinking: String,
    thinking_seen: bool,
}

impl Batch {
    fn flush(&mut self, shared: &Shared, sid: &str) {
        if self.thinking_seen {
            self.thinking_seen = false;
            shared.emit(
                "agent.text",
                sid,
                json!({ "text": std::mem::take(&mut self.thinking), "thinking": true }),
            );
        }
        if !self.text.is_empty() {
            shared.emit(
                "agent.text",
                sid,
                json!({ "text": std::mem::take(&mut self.text) }),
            );
        }
    }
}

async fn stream_turn(
    shared: &Arc<Shared>,
    session: &Arc<Session>,
    plan: &TurnPlan,
    request: &ChatRequest,
    cancel: &CancellationToken,
) -> Result<Turn, End> {
    let sid = session.id.as_str();
    let payload = serde_json::to_string(request).map_err(|e| {
        failed(
            "internal",
            format!("could not encode the request: {e}"),
            false,
        )
    })?;
    let mut audited = false;
    let mut write_request = |request_id: Option<String>| -> Result<(), AgentError> {
        if audited {
            return Ok(());
        }
        audited = true;
        audit(
            session,
            AuditEvent::Request {
                provider: plan.provider_id.clone(),
                model: plan.model.clone(),
                request_id,
                payload_sha256: payload_hash(payload.as_bytes()),
                payload_bytes: payload.len() as u64,
                redactions: plan.redaction_count,
                payload: Some(payload.clone()),
            },
        )
    };

    let mut stream = plan.provider.stream(request);
    let mut batch = Batch {
        text: String::new(),
        thinking: String::new(),
        thinking_seen: false,
    };
    let mut deadline: Option<tokio::time::Instant> = None;
    let mut blocks: Vec<ContentBlock> = Vec::new();
    let mut text = String::new();
    let mut calls: Vec<ToolUseView> = Vec::new();
    let mut usage = None;
    let mut stop = None;
    loop {
        let due = deadline;
        let wait = async move {
            match due {
                Some(at) => tokio::time::sleep_until(at).await,
                None => std::future::pending::<()>().await,
            }
        };
        let next = tokio::select! {
            biased;
            () = cancel.cancelled() => {
                batch.flush(shared, sid);
                write_request(None)?;
                return Err(End::Cancelled);
            }
            () = wait => {
                batch.flush(shared, sid);
                deadline = None;
                continue;
            }
            next = tokio::time::timeout(
                shared.limits.idle,
                futures_util::StreamExt::next(&mut stream),
            ) => next,
        };
        let event = match next {
            Err(_) => {
                batch.flush(shared, sid);
                write_request(None)?;
                return Err(failed("timeout", "the provider stopped responding", true));
            }
            Ok(None) => break,
            Ok(Some(event)) => event,
        };
        match event {
            ProviderEvent::Started { request_id, .. } => write_request(request_id)?,
            ProviderEvent::TextDelta { text: delta } => {
                text.push_str(&delta);
                batch.text.push_str(&delta);
                deadline.get_or_insert_with(|| tokio::time::Instant::now() + shared.limits.flush);
            }
            ProviderEvent::ThinkingDelta { text: delta } => {
                if plan.include_thinking {
                    batch.thinking.push_str(&delta);
                }
                batch.thinking_seen = true;
                deadline.get_or_insert_with(|| tokio::time::Instant::now() + shared.limits.flush);
            }
            ProviderEvent::Block { block } => blocks.push(block),
            ProviderEvent::ToolUse { id, name, input } => {
                calls.push(ToolUseView { id, name, input })
            }
            ProviderEvent::Usage { usage: u } => usage = Some(u),
            ProviderEvent::Done { stop_reason } => {
                stop = Some(stop_reason);
                break;
            }
            ProviderEvent::Error { error } => {
                batch.flush(shared, sid);
                write_request(error.request_id.clone())?;
                return Err(End::Failed(error));
            }
        }
    }
    batch.flush(shared, sid);
    write_request(None)?;
    let Some(stop) = stop else {
        return Err(failed(
            "stream_truncated",
            "the response ended before it was complete",
            true,
        ));
    };
    if let Some(u) = &usage {
        ledger(shared, session, plan, u);
    }
    // Real providers send every block; recordings may send only the text and tool calls as
    // events. Whatever is missing is rebuilt, so history is always complete.
    if !blocks.iter().any(|b| b.kind() == BlockKind::Text) && !text.is_empty() {
        blocks.push(ContentBlock::text(&text).map_err(|e| End::from(AgentError::from(e)))?);
    }
    for call in &calls {
        let present = blocks
            .iter()
            .filter_map(ContentBlock::as_tool_use)
            .any(|b| b.id == call.id);
        if !present {
            let wire = plan.names.wire(&call.name).unwrap_or(&call.name);
            blocks.push(
                ContentBlock::tool_use(&call.id, wire, &call.input)
                    .map_err(|e| End::from(AgentError::from(e)))?,
            );
        }
    }
    Ok(Turn {
        content: blocks,
        calls,
        usage,
        stop,
    })
}

// ---- tools --------------------------------------------------------------------------------

/// What running one call produced.
struct ToolOutcome {
    content: String,
    is_error: bool,
    cancelled: bool,
    /// `ask_user`: the model's turn ends after this round.
    asks: bool,
    /// A proposal was queued.
    edited: bool,
}

impl ToolOutcome {
    fn new(content: String, is_error: bool) -> Self {
        Self {
            content,
            is_error,
            cancelled: false,
            asks: false,
            edited: false,
        }
    }

    fn error(message: &str) -> Self {
        Self::new(message.to_owned(), true)
    }
}

async fn run_tools(
    shared: &Arc<Shared>,
    session: &Arc<Session>,
    plan: &TurnPlan,
    calls: &[ToolUseView],
    cancel: &CancellationToken,
    guard: &mut RepeatGuard,
) -> (Vec<ContentBlock>, bool, bool) {
    let mut results = Vec::with_capacity(calls.len());
    let mut cancelled = false;
    let mut asked = false;
    for call in calls {
        let outcome = if cancelled {
            // The history must stay valid: every tool call gets a result, even a skipped one.
            ToolOutcome::error("Cancelled: the author stopped the session before this ran.")
        } else {
            run_tool(shared, session, plan, call, cancel, guard).await
        };
        cancelled |= outcome.cancelled;
        asked |= outcome.asks;
        if outcome.edited {
            guard.reset();
        }
        if let Ok(block) = ContentBlock::tool_result(&call.id, &outcome.content, outcome.is_error) {
            results.push(block);
        }
    }
    (results, cancelled, asked)
}

fn summary(content: &str) -> String {
    let first = content.lines().next().unwrap_or("");
    let (head, cut) = clip_chars(first, 160);
    if cut { format!("{head}...") } else { head }
}

/// Emits the `agent.tool` events of one call: the call and its result are one updating row in
/// the UI (`running`, then `done` or `error`).
struct ToolEvents<'a> {
    shared: &'a Shared,
    sid: &'a str,
    call: &'a ToolUseView,
    risk: manang_agent_core::RiskClass,
}

impl ToolEvents<'_> {
    fn emit(&self, status: &str, summary: &str, extra: Value) {
        let mut payload = json!({
            "id": self.call.id, "name": self.call.name, "risk": self.risk, "status": status,
            "args": self.call.input, "summary": summary,
        });
        if let (Some(map), Value::Object(more)) = (payload.as_object_mut(), extra) {
            map.extend(more);
        }
        self.shared.emit("agent.tool", self.sid, payload);
    }
}

async fn run_tool(
    shared: &Arc<Shared>,
    session: &Arc<Session>,
    plan: &TurnPlan,
    call: &ToolUseView,
    cancel: &CancellationToken,
    guard: &mut RepeatGuard,
) -> ToolOutcome {
    let sid = session.id.as_str();
    let known = plan.tools.iter().any(|t| t.name == call.name);
    let info = shared.risk.get(&call.name).copied();
    // Only tools that were offered (and so have a class) can run, whatever the model asked for.
    let risk = info.map_or(manang_agent_core::RiskClass::Exec, |i| i.risk);
    let events = ToolEvents {
        shared,
        sid,
        call,
        risk,
    };
    events.emit("running", "", json!({}));
    let finish = |out: ToolOutcome, extra: Value| {
        events.emit(
            if out.is_error { "error" } else { "done" },
            &summary(&out.content),
            extra,
        );
        out
    };
    if !known {
        return finish(
            ToolOutcome::error(&format!(
                "There is no tool named `{}`; use one of the tools you were given.",
                call.name
            )),
            json!({}),
        );
    }
    let args_text = call.input.to_string();
    if let Err(e) = audit(
        session,
        AuditEvent::ToolCall {
            tool: call.name.clone(),
            risk,
            arguments_sha256: payload_hash(args_text.as_bytes()),
        },
    ) {
        return finish(ToolOutcome::error(&e.message), json!({}));
    }

    let (remembered, tainted) = {
        let inner = session.inner.lock();
        (inner.remembered.clone(), inner.tainted)
    };
    match gate(info.as_ref(), &remembered, tainted) {
        Gate::Auto => {}
        Gate::Deny(why) => {
            let _ = audit(
                session,
                AuditEvent::Approval {
                    tool: call.name.clone(),
                    approved: false,
                    by: "policy".to_owned(),
                },
            );
            return finish(ToolOutcome::error(&why), json!({}));
        }
        Gate::Ask { scope } => {
            let approval_id = format!("{sid}/{}", call.id);
            let (tx, rx) = tokio::sync::oneshot::channel();
            shared.approvals.lock().insert(
                approval_id.clone(),
                PendingApproval {
                    tx,
                    session_id: sid.to_owned(),
                },
            );
            shared.emit(
                "agent.approval",
                sid,
                json!({
                    "call_id": approval_id, "tool_call_id": call.id, "tool": call.name,
                    "risk": risk, "scope": scope, "args": call.input,
                    "host": info.and_then(|i| i.host),
                    "after_untrusted_content": tainted,
                }),
            );
            let decision = tokio::select! {
                biased;
                () = cancel.cancelled() => {
                    shared.approvals.lock().remove(&approval_id);
                    let mut out = ToolOutcome::error("Cancelled: the author stopped the session.");
                    out.cancelled = true;
                    return finish(out, json!({}));
                }
                answer = rx => answer.unwrap_or(Decision::Deny),
                () = tokio::time::sleep(shared.limits.approval) => {
                    shared.approvals.lock().remove(&approval_id);
                    Decision::Deny
                }
            };
            let approved = decision != Decision::Deny;
            if let Err(e) = audit(
                session,
                AuditEvent::Approval {
                    tool: call.name.clone(),
                    approved,
                    by: "user".to_owned(),
                },
            ) {
                return finish(ToolOutcome::error(&e.message), json!({}));
            }
            if !approved {
                return finish(
                    ToolOutcome::error(
                        "The author declined this action. Do not try it again; continue without it.",
                    ),
                    json!({}),
                );
            }
            if decision == Decision::AllowSession {
                session.inner.lock().remembered.insert(scope);
            }
        }
    }

    let cacheable = risk == manang_agent_core::RiskClass::Read;
    if guard.repeated(call, cacheable) {
        return finish(
            ToolOutcome::new("Same as the earlier result (unchanged).".to_owned(), false),
            json!({}),
        );
    }

    // CPU and kernel work: Rayon, awaited through a oneshot.
    let kernel = match shared.kernel() {
        Ok(k) => k,
        Err(e) => return finish(ToolOutcome::error(&e.message), json!({})),
    };
    let (tx, rx) = tokio::sync::oneshot::channel();
    let name = call.name.clone();
    let input = call.input.clone();
    shared.pool.spawn(move || {
        let _ = tx.send(kernel.call_agent_tool(&name, input));
    });
    let result = tokio::select! {
        biased;
        () = cancel.cancelled() => {
            let mut out = ToolOutcome::error("Cancelled: the author stopped the session.");
            out.cancelled = true;
            return finish(out, json!({}));
        }
        r = rx => r,
        () = tokio::time::sleep(shared.limits.tool) => {
            return finish(ToolOutcome::error("The tool took too long and was stopped. Try a narrower request."), json!({}));
        }
    };
    let (outcome, extra) = match result {
        Ok(Ok(payload)) => ok_outcome(shared, session, plan, risk, payload),
        Ok(Err(error)) => (ToolOutcome::new(error_text(&error), true), json!({})),
        Err(_) => (ToolOutcome::error("The tool did not answer."), json!({})),
    };
    if info.is_some_and(|i| i.untrusted_output) {
        session.inner.lock().tainted = true;
    }
    finish(outcome, extra)
}

fn error_text(error: &AgentToolError) -> String {
    match error {
        AgentToolError::InvalidInput { issues, .. } => {
            let list: Vec<String> = issues
                .iter()
                .map(|i| {
                    let at = if i.path.is_empty() { "input" } else { &i.path };
                    format!("{at}: {}", i.message)
                })
                .collect();
            format!(
                "Invalid arguments: {}. Fix them and call the tool again.",
                list.join("; ")
            )
        }
        AgentToolError::UnknownTool(name) => format!("There is no tool named `{name}`."),
        AgentToolError::Kernel(KernelError::Plugin(PluginError::Failed(message))) => {
            message.clone()
        }
        AgentToolError::Kernel(other) => format!("The tool failed: {other}"),
    }
}

fn ok_outcome(
    shared: &Shared,
    session: &Session,
    plan: &TurnPlan,
    risk: manang_agent_core::RiskClass,
    payload: Payload,
) -> (ToolOutcome, Value) {
    let value = match payload {
        Payload::Json(v) => v,
        Payload::Empty => Value::String("Done.".to_owned()),
        Payload::Binary(bytes) => Value::String(format!("Done ({} bytes).", bytes.len())),
    };
    // ask_user: the question goes to the author, the model's turn ends.
    if risk == manang_agent_core::RiskClass::Interaction
        && let Some(ask) = value.get("ask")
    {
        let mut out = ToolOutcome::new(
            "Question shown to the author. Their answer arrives as the next message; stop now."
                .to_owned(),
            false,
        );
        out.asks = true;
        return (out, json!({ "ask": ask }));
    }
    // Only a `propose` tool can hand the author a proposal; the model sees just its id.
    if risk == manang_agent_core::RiskClass::Propose
        && let Some(proposal) = value.get("proposal").and_then(Value::as_object)
    {
        let kind = proposal
            .get("kind")
            .and_then(Value::as_str)
            .unwrap_or("text_edit")
            .to_owned();
        if matches!(kind.as_str(), "text_edit" | "scene_patch") {
            let id = shared.new_id("p");
            let seq = shared.next_seq();
            shared.proposals.lock().insert(
                id.clone(),
                StoredProposal {
                    seq,
                    session_id: session.id.clone(),
                    store: session.store.clone(),
                    kind: kind.clone(),
                    payload: Value::Object(proposal.clone()),
                    state: ProposalState::Pending,
                },
            );
            shared.emit(
                "agent.proposal",
                &session.id,
                json!({ "proposal_id": id, "kind": kind, "payload": proposal }),
            );
            let path = proposal.get("path").and_then(Value::as_str).unwrap_or("");
            let n = proposal
                .get("edits")
                .and_then(Value::as_array)
                .map_or(1, Vec::len);
            let removed = value.get("removed").and_then(Value::as_u64).unwrap_or(0);
            let added = value.get("added").and_then(Value::as_u64).unwrap_or(0);
            let mut out = ToolOutcome::new(
                format!(
                    "Proposal {id} queued for {path}: {n} edit{}, -{removed} +{added} characters. Markup check passed.",
                    if n == 1 { "" } else { "s" }
                ),
                false,
            );
            out.edited = true;
            return (out, json!({ "proposal_id": id }));
        }
    }
    let text = match value {
        Value::String(s) => s,
        other => other.to_string(),
    };
    let (clipped, cut) = clip_chars(&text, crate::tools::RESULT_CAP);
    let safe = redact(&clipped, &plan.redaction).text;
    (
        ToolOutcome::new(
            if cut {
                format!(
                    "{safe}\n[output cut at {} characters]",
                    crate::tools::RESULT_CAP
                )
            } else {
                safe
            },
            false,
        ),
        json!({}),
    )
}
