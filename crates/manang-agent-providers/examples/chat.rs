//! Try any provider from a terminal (not shipped in the app).
//!
//! ```text
//! DEEPSEEK_API_KEY=... cargo run -p manang-agent-providers --example chat -- --preset deepseek "hello"
//! cargo run -p manang-agent-providers --example chat -- --preset ollama --model llama3.2 "hello"
//! cargo run -p manang-agent-providers --example chat -- --preset custom --base-url http://host:8000/v1 --model m "hello"
//! cargo run -p manang-agent-providers --example chat -- --list
//! ```
//!
//! Options: `--preset <id>` (default `anthropic`), `--model <id>`,
//! `--base-url <url>`, `--key-env <VAR>` (default: the preset's variable),
//! `--offline` (loopback only), `--max-tokens <n>`, `--list`.
//! Text goes to stdout, reasoning and the usage/cost line to stderr. The key
//! is read from the environment and never printed.

use futures_util::StreamExt;
use manang_agent_core::{
    ChatRequest, Message, PriceTable, Provider, ProviderEvent, ProviderKind, Usage, preset_by_id,
    presets,
};
use manang_agent_providers::{
    AnthropicConfig, AnthropicProvider, ApiKey, OpenAiCompatConfig, OpenAiCompatProvider,
    ReqwestTransport, TransportConfig,
};
use std::io::Write;
use std::process::ExitCode;
use std::sync::Arc;

struct Args {
    preset: String,
    model: Option<String>,
    base_url: Option<String>,
    key_env: Option<String>,
    offline: bool,
    max_tokens: u32,
    list: bool,
    prompt: String,
}

fn parse_args() -> Result<Args, String> {
    let mut args = Args {
        preset: "anthropic".to_owned(),
        model: None,
        base_url: None,
        key_env: None,
        offline: false,
        max_tokens: 2048,
        list: false,
        prompt: String::new(),
    };
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        let mut value = |name: &str| it.next().ok_or_else(|| format!("{name} needs a value"));
        match arg.as_str() {
            "--preset" => args.preset = value("--preset")?,
            "--model" => args.model = Some(value("--model")?),
            "--base-url" => args.base_url = Some(value("--base-url")?),
            "--key-env" => args.key_env = Some(value("--key-env")?),
            "--max-tokens" => {
                args.max_tokens = value("--max-tokens")?
                    .parse()
                    .map_err(|_| "--max-tokens must be a number".to_owned())?;
            }
            "--offline" => args.offline = true,
            "--list" => args.list = true,
            other if other.starts_with("--") => return Err(format!("unknown option {other}")),
            other => {
                if !args.prompt.is_empty() {
                    args.prompt.push(' ');
                }
                args.prompt.push_str(other);
            }
        }
    }
    Ok(args)
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> Result<(), String> {
    let args = parse_args()?;
    if args.list {
        for p in presets() {
            println!(
                "{:<11} {:<26} {}{}",
                p.id,
                p.display_name,
                p.base_url.as_deref().unwrap_or("(you supply --base-url)"),
                p.key_env_var
                    .as_deref()
                    .map(|v| format!("   key: ${v}"))
                    .unwrap_or_default()
            );
        }
        return Ok(());
    }
    if args.prompt.is_empty() {
        return Err("give a prompt, e.g. --preset deepseek \"hello\" (or --list)".to_owned());
    }
    let preset =
        preset_by_id(&args.preset).ok_or_else(|| format!("unknown preset `{}`", args.preset))?;
    let key = {
        let var = args.key_env.clone().or_else(|| preset.key_env_var.clone());
        match var.as_deref().map(|v| (v, std::env::var(v))) {
            Some((_, Ok(value))) if !value.is_empty() => Some(ApiKey::new(&value)),
            Some((name, _)) if preset.key_required => {
                return Err(format!(
                    "set ${name} (the key is read from the environment)"
                ));
            }
            _ if preset.key_required => return Err("this preset needs --key-env <VAR>".to_owned()),
            _ => None,
        }
    };
    let model = args
        .model
        .clone()
        .or_else(|| preset.models.first().cloned())
        .ok_or_else(|| "this preset lists no default model: pass --model".to_owned())?;
    let transport = ReqwestTransport::new(&TransportConfig {
        loopback_only: args.offline,
        ..TransportConfig::default()
    })
    .map_err(|e| e.to_string())?;
    let provider: Box<dyn Provider> = match preset.kind {
        ProviderKind::Anthropic => {
            let mut config = AnthropicConfig::new(key.ok_or("an Anthropic key is required")?);
            if let Some(url) = args.base_url.as_deref().or(preset.base_url.as_deref()) {
                config.base_url = url.to_owned();
            }
            config.offline = args.offline;
            Box::new(AnthropicProvider::new(config, Arc::new(transport)))
        }
        ProviderKind::OpenAiCompat => {
            let mut config =
                OpenAiCompatConfig::from_preset(&preset, args.base_url.as_deref(), key)?;
            config.offline = args.offline;
            Box::new(OpenAiCompatProvider::new(config, Arc::new(transport)))
        }
    };
    let request = ChatRequest {
        model: model.clone(),
        system: None,
        messages: vec![Message::user_text(&args.prompt).map_err(|e| e.to_string())?],
        tools: vec![],
        max_tokens: args.max_tokens,
        cache_breakpoints: vec![],
    };
    let mut stream = provider.stream(&request);
    let mut usage = Usage::default();
    let mut failed = None;
    while let Some(event) = stream.next().await {
        match event {
            ProviderEvent::TextDelta { text } => {
                print!("{text}");
                let _ = std::io::stdout().flush();
            }
            ProviderEvent::ThinkingDelta { text } => {
                eprint!("\x1b[2m{text}\x1b[0m");
            }
            ProviderEvent::Usage { usage: u } => usage = u,
            ProviderEvent::Error { error } => {
                failed = Some(format!(
                    "{} ({}): {}{}",
                    error.code,
                    if error.retryable {
                        "retryable"
                    } else {
                        "not retryable"
                    },
                    error.message,
                    error
                        .request_id
                        .map(|r| format!(" [request {r}]"))
                        .unwrap_or_default()
                ));
            }
            _ => {}
        }
    }
    println!();
    let cost = PriceTable::builtin().estimate(&model, &usage).map_or_else(
        || "cost unknown for this model".to_owned(),
        |c| format!("{} (estimate)", c.display()),
    );
    eprintln!(
        "usage: {} in, {} out, {} cached, {} cache-write; {cost}",
        usage.input, usage.output, usage.cache_read, usage.cache_write
    );
    failed.map_or(Ok(()), Err)
}
