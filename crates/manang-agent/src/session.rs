//! The orchestrator loop: model turn, tool calls, results, next turn.
//!
//! Tokio is used only to stream, to wait (timers, cancellation, approvals) and to await tool
//! results. Every tool call is CPU or kernel work: it is handed to the Rayon pool and awaited
//! through a oneshot, never run on a Tokio worker (AGENTS.md section 2.3).
//!
//! History is append-only and lossless: the blocks a provider produced are stored and replayed
//! verbatim (thinking blocks and their signatures included).

use crate::error::AgentError;
use crate::risk::{Decision, Gate, gate};
use crate::state::{PendingApproval, ProposalState, Session, Shared, StoredProposal};
use crate::storage::now_ms;
use manang_agent_core::{
    AuditEntry, AuditEvent, BlockKind, CacheBreakpoint, ChatRequest, ContentBlock, Message,
    PriceTable, Provider, ProviderError, ProviderEvent, RedactionConfig, Role, StopReason,
    ToolNameMap, ToolSpec, ToolUseView, TranscriptEntry, Usage, payload_hash, redact,
};
use manang_kernel::{AgentToolError, KernelError, Payload, PluginError};
use serde_json::{Value, json};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

/// Everything a session task needs, fixed when the session starts.
pub struct TurnPlan {
    pub provider: Arc<dyn Provider>,
    pub provider_id: String,
    pub model: String,
    pub system: String,
    pub tools: Vec<ToolSpec>,
    pub names: ToolNameMap,
    pub max_tokens: u32,
    pub max_iterations: u32,
    pub max_total_tokens: u64,
    pub include_thinking: bool,
    pub cache: bool,
    pub redaction: RedactionConfig,
    pub redaction_count: u32,
    pub prices: PriceTable,
}

impl TurnPlan {
    /// The request for the current history.
    pub fn request(&self, history: &[Message]) -> ChatRequest {
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
            messages: history.to_vec(),
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

/// Runs a session until it ends, then reports how (events, meta, pending approvals).
pub async fn run(
    shared: Arc<Shared>,
    session: Arc<Session>,
    plan: TurnPlan,
    cancel: CancellationToken,
) {
    let sid = session.id.clone();
    let end = drive(&shared, &session, &plan, &cancel).await;
    // Anything still waiting for the user belongs to a session that is over.
    shared.approvals.lock().retain(|_, p| p.session_id != sid);
    // The session is idle (and its meta on disk) before the UI hears that it is done.
    {
        let mut inner = session.inner.lock();
        inner.running = false;
        inner.cancel = None;
    }
    let meta_error = crate::plugin::write_meta(&session, "idle").err();
    match end {
        End::Done(reason) => shared.emit("agent.done", &sid, json!({ "stop_reason": reason })),
        End::Cancelled => {
            shared.emit("agent.cancelled", &sid, json!({}));
            shared.emit("agent.done", &sid, json!({ "stop_reason": "cancelled" }));
        }
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

async fn drive(
    shared: &Arc<Shared>,
    session: &Arc<Session>,
    plan: &TurnPlan,
    cancel: &CancellationToken,
) -> End {
    for _ in 0..plan.max_iterations {
        if cancel.is_cancelled() {
            return End::Cancelled;
        }
        let request = plan.request(&session.inner.lock().history);
        let turn = match stream_turn(shared, session, plan, &request, cancel).await {
            Ok(turn) => turn,
            Err(end) => return end,
        };
        if let Some(usage) = turn.usage
            && let Err(e) = record_usage(shared, session, plan, &usage)
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
        let (results, cancelled) = run_tools(shared, session, plan, &turn.calls, cancel).await;
        if let Err(e) = commit(session, Message::new(Role::User, results)) {
            return e.into();
        }
        if cancelled {
            return End::Cancelled;
        }
        let used = {
            let inner = session.inner.lock();
            inner.usage.input + inner.usage.output
        };
        if used >= plan.max_total_tokens {
            return End::Done("token_cap".to_owned());
        }
    }
    End::Done("iteration_cap".to_owned())
}

/// Appends a message to the history and the transcript.
pub fn commit(session: &Session, message: Message) -> Result<(), AgentError> {
    let seq = {
        let mut inner = session.inner.lock();
        inner.seq += 1;
        inner.history.push(message.clone());
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
    shared: &Shared,
    session: &Session,
    plan: &TurnPlan,
    usage: &Usage,
) -> Result<(), AgentError> {
    let cost = plan
        .prices
        .estimate(&plan.model, usage)
        .map(|c| c.micro_usd);
    let (total, total_cost) = {
        let mut inner = session.inner.lock();
        inner.usage = inner.usage.add(usage);
        inner.cost_micro = inner.cost_micro.saturating_add(cost.unwrap_or(0));
        inner.turns += 1;
        (inner.usage, inner.cost_micro)
    };
    shared.emit(
        "agent.usage",
        &session.id,
        json!({
            "model": plan.model,
            "input": usage.input, "output": usage.output,
            "cache_read": usage.cache_read, "cache_write": usage.cache_write,
            "est_cost_micro_usd": cost,
            "total": {
                "input": total.input, "output": total.output,
                "cache_read": total.cache_read, "cache_write": total.cache_write,
                "est_cost_micro_usd": total_cost,
            },
        }),
    );
    audit(
        session,
        AuditEvent::Usage {
            model: plan.model.clone(),
            usage: *usage,
            cost_micro_usd: cost,
        },
    )?;
    // The global ledger holds numbers only: no text, no key.
    shared.global.append_usage(&json!({
        "version": crate::storage::FORMAT_VERSION,
        "ts_ms": now_ms(),
        "session": session.id,
        "provider": plan.provider_id,
        "model": plan.model,
        "input": usage.input, "output": usage.output,
        "cache_read": usage.cache_read, "cache_write": usage.cache_write,
        "est_cost_micro_usd": cost,
    }))
}

struct Turn {
    content: Vec<ContentBlock>,
    calls: Vec<ToolUseView>,
    usage: Option<Usage>,
    stop: StopReason,
}

/// Streaming deltas are sent in batches, so a fast model does not flood the IPC.
struct Batch {
    text: String,
    thinking: String,
}

impl Batch {
    fn flush(&mut self, shared: &Shared, sid: &str) {
        if !self.thinking.is_empty() {
            shared.emit(
                "agent.thinking-delta",
                sid,
                json!({ "text": std::mem::take(&mut self.thinking) }),
            );
        }
        if !self.text.is_empty() {
            shared.emit(
                "agent.text-delta",
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
                    deadline
                        .get_or_insert_with(|| tokio::time::Instant::now() + shared.limits.flush);
                }
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

/// What running one call produced.
struct ToolOutcome {
    content: String,
    is_error: bool,
    cancelled: bool,
}

impl ToolOutcome {
    fn error(code: &str, message: &str) -> Self {
        Self {
            content: format!("[{code}] {message}"),
            is_error: true,
            cancelled: false,
        }
    }
}

async fn run_tools(
    shared: &Arc<Shared>,
    session: &Arc<Session>,
    plan: &TurnPlan,
    calls: &[ToolUseView],
    cancel: &CancellationToken,
) -> (Vec<ContentBlock>, bool) {
    let mut results = Vec::with_capacity(calls.len());
    let mut cancelled = false;
    for call in calls {
        let outcome = if cancelled {
            ToolOutcome::error("cancelled", "the user stopped the session before this ran")
        } else {
            run_tool(shared, session, plan, call, cancel).await
        };
        cancelled |= outcome.cancelled;
        if let Ok(block) = ContentBlock::tool_result(&call.id, &outcome.content, outcome.is_error) {
            results.push(block);
        }
    }
    (results, cancelled)
}

fn summary(content: &str) -> String {
    let (head, cut) = crate::text::clip_chars(content, 160);
    if cut { format!("{head}...") } else { head }
}

async fn run_tool(
    shared: &Arc<Shared>,
    session: &Arc<Session>,
    plan: &TurnPlan,
    call: &ToolUseView,
    cancel: &CancellationToken,
) -> ToolOutcome {
    let sid = session.id.as_str();
    let known = plan.tools.iter().any(|t| t.name == call.name);
    let info = shared.risk.get(&call.name).copied();
    // Only tools that were offered (and so have a class) can run, whatever the model asked for.
    let risk = info.map_or(manang_agent_core::RiskClass::Exec, |i| i.risk);
    shared.emit(
        "agent.tool-call",
        sid,
        json!({ "id": call.id, "name": call.name, "risk": risk, "args": call.input }),
    );
    if !known {
        let out = ToolOutcome::error(
            "unknown_tool",
            &format!(
                "there is no tool named `{}`; use one of the tools you were given",
                call.name
            ),
        );
        shared.emit(
            "agent.tool-result",
            sid,
            json!({ "id": call.id, "is_error": true, "summary": summary(&out.content) }),
        );
        return out;
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
        return ToolOutcome::error(e.code, &e.message);
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
            return ToolOutcome::error("denied", &why);
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
                "agent.approval-requested",
                sid,
                json!({
                    "call_id": approval_id, "tool_call_id": call.id, "tool": call.name,
                    "risk": risk, "scope": scope, "args": call.input,
                    "after_untrusted_content": tainted,
                }),
            );
            let decision = tokio::select! {
                biased;
                () = cancel.cancelled() => {
                    shared.approvals.lock().remove(&approval_id);
                    let mut out = ToolOutcome::error("cancelled", "the user stopped the session");
                    out.cancelled = true;
                    return out;
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
                return ToolOutcome::error(e.code, &e.message);
            }
            if !approved {
                let out = ToolOutcome::error("denied", "the user declined this action");
                shared.emit(
                    "agent.tool-result",
                    sid,
                    json!({ "id": call.id, "is_error": true, "summary": summary(&out.content) }),
                );
                return out;
            }
            if decision == Decision::AllowSession {
                session.inner.lock().remembered.insert(scope);
            }
        }
    }

    // CPU and kernel work: Rayon, awaited through a oneshot.
    let kernel = match shared.kernel() {
        Ok(k) => k,
        Err(e) => return ToolOutcome::error(e.code, &e.message),
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
            let mut out = ToolOutcome::error("cancelled", "the user stopped the session");
            out.cancelled = true;
            return out;
        }
        r = rx => r,
        () = tokio::time::sleep(shared.limits.tool) => {
            let out = ToolOutcome::error("timeout", "the tool took too long");
            shared.emit(
                "agent.tool-result", sid,
                json!({ "id": call.id, "is_error": true, "summary": summary(&out.content) }),
            );
            return out;
        }
    };
    let outcome = match result {
        Ok(Ok(payload)) => ok_outcome(shared, session, plan, risk, payload),
        Ok(Err(error)) => ToolOutcome {
            content: error_text(&error),
            is_error: true,
            cancelled: false,
        },
        Err(_) => ToolOutcome::error("unavailable", "the tool did not answer"),
    };
    if info.is_some_and(|i| i.untrusted_output) {
        session.inner.lock().tainted = true;
    }
    shared.emit(
        "agent.tool-result",
        sid,
        json!({ "id": call.id, "is_error": outcome.is_error, "summary": summary(&outcome.content) }),
    );
    outcome
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
                "[invalid_input] {}. Fix the arguments and call the tool again.",
                list.join("; ")
            )
        }
        AgentToolError::UnknownTool(name) => format!("[unknown_tool] no tool named `{name}`"),
        AgentToolError::Kernel(KernelError::Plugin(PluginError::Failed(message))) => {
            message.clone()
        }
        AgentToolError::Kernel(other) => format!("[tool_failed] {other}"),
    }
}

fn ok_outcome(
    shared: &Shared,
    session: &Session,
    plan: &TurnPlan,
    risk: manang_agent_core::RiskClass,
    payload: Payload,
) -> ToolOutcome {
    let value = match payload {
        Payload::Json(v) => v,
        Payload::Empty => json!({ "ok": true }),
        Payload::Binary(bytes) => json!({ "ok": true, "binary_bytes": bytes.len() }),
    };
    // Only a `propose` tool can hand the user a proposal; the model sees just its id.
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
            shared.proposals.lock().insert(
                id.clone(),
                StoredProposal {
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
            let message = value.get("message").cloned().unwrap_or(Value::Null);
            return ToolOutcome {
                content: json!({ "ok": true, "proposal_id": id, "message": message }).to_string(),
                is_error: false,
                cancelled: false,
            };
        }
    }
    let text = value.to_string();
    let (clipped, cut) = crate::text::clip_chars(&text, 24_000);
    let safe = redact(&clipped, &plan.redaction).text;
    ToolOutcome {
        content: if cut {
            format!("{safe}\n[output cut at 24000 characters]")
        } else {
            safe
        },
        is_error: false,
        cancelled: false,
    }
}
