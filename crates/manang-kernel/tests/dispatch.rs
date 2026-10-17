//! End-to-end behaviour of the kernel with in-process plugins.

use std::sync::Arc;

use manang_kernel::{
    Access, Capability, Context, Hook, Kernel, KernelError, Manifest, Payload, Plugin, PluginError,
};
use serde_json::json;

const NATIVE: &str = r#"
[plugin]
name = "echo"
version = "0.1.0"
tier = "native"

[permissions]
buffer = ["read:active"]

[contributes]
hooks = ["on_compile_pre", "on_lint"]

[[contributes.commands]]
id = "echo.say"
title = "Say"

[[contributes.commands]]
id = "echo.write"
title = "Write buffer"
"#;

struct Echo;

impl Plugin for Echo {
    fn invoke(
        &self,
        ctx: &Context<'_>,
        command: &str,
        input: Payload,
    ) -> Result<Payload, PluginError> {
        match command {
            "echo.say" => Ok(input),
            "echo.write" => {
                ctx.require(&Capability::Buffer {
                    access: Access::Write,
                    target: "active".into(),
                })?;
                Ok(Payload::Empty)
            }
            other => Err(PluginError::UnknownCommand(other.to_owned())),
        }
    }

    fn on_hook(&self, _: &Context<'_>, hook: Hook, input: Payload) -> Result<Payload, PluginError> {
        match (hook, input) {
            (Hook::OnCompilePre, Payload::Json(v)) => Ok(Payload::Json(json!({ "wrapped": v }))),
            (_, other) => Ok(other),
        }
    }
}

fn manifest(src: &str) -> Manifest {
    Manifest::from_toml(src).expect("valid manifest")
}

fn kernel_with_echo() -> Kernel {
    let k = Kernel::new();
    k.register(manifest(NATIVE), Arc::new(Echo))
        .expect("registers");
    k
}

#[test]
fn invokes_a_registered_command() {
    let k = kernel_with_echo();
    let out = k.invoke("echo.say", Payload::Json(json!(1))).expect("ok");
    assert_eq!(out, Payload::Json(json!(1)));
}

#[test]
fn unknown_command_is_an_error() {
    let k = kernel_with_echo();
    assert!(matches!(
        k.invoke("nope.run", Payload::Empty),
        Err(KernelError::UnknownCommand(_))
    ));
}

#[test]
fn undeclared_capability_is_denied() {
    let k = kernel_with_echo();
    // The manifest grants read:active only, so a write must be refused.
    assert!(matches!(
        k.invoke("echo.write", Payload::Empty),
        Err(KernelError::Plugin(PluginError::CapabilityDenied { .. }))
    ));
}

#[test]
fn duplicate_plugin_and_command_are_rejected_atomically() {
    let k = kernel_with_echo();
    assert!(matches!(
        k.register(manifest(NATIVE), Arc::new(Echo)),
        Err(KernelError::DuplicatePlugin(_))
    ));
    assert_eq!(k.commands().len(), 2, "failed register left no residue");
}

#[test]
fn commands_are_listed_sorted_and_removed_on_unregister() {
    let k = kernel_with_echo();
    let ids: Vec<_> = k.commands().into_iter().map(|c| c.id).collect();
    assert_eq!(ids, ["echo.say", "echo.write"]);
    k.unregister("echo").expect("registered");
    assert!(k.commands().is_empty());
    assert!(matches!(
        k.invoke("echo.say", Payload::Empty),
        Err(KernelError::UnknownCommand(_))
    ));
    assert!(matches!(
        k.unregister("echo"),
        Err(KernelError::UnknownPlugin(_))
    ));
}

#[test]
fn pipeline_hook_chains_and_fanout_collects() {
    let k = kernel_with_echo();
    let out = k
        .run_hook(Hook::OnCompilePre, Payload::Json(json!("src")))
        .expect("ok");
    assert_eq!(
        out,
        [("echo".into(), Payload::Json(json!({"wrapped": "src"})))]
    );

    let out = k.run_hook(Hook::OnLint, Payload::Empty).expect("ok");
    assert_eq!(out.len(), 1);

    // No subscribers: empty result, input untouched semantics.
    assert!(
        k.run_hook(Hook::OnCompilePost, Payload::Empty)
            .expect("ok")
            .is_empty()
    );
}
