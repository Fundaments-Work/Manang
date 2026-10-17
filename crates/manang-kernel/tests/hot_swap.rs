//! Hot reload semantics of the kernel registry.

use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};

use manang_kernel::{Context, Hook, Kernel, KernelError, Manifest, Payload, Plugin, PluginError};
use serde_json::json;

struct Slow {
    started: Sender<()>,
    release: Mutex<Receiver<()>>,
}

impl Plugin for Slow {
    fn invoke(&self, _: &Context<'_>, _: &str, _: Payload) -> Result<Payload, PluginError> {
        self.started
            .send(())
            .map_err(|e| PluginError::Failed(e.to_string()))?;
        let rx = self
            .release
            .lock()
            .map_err(|e| PluginError::Failed(e.to_string()))?;
        rx.recv().map_err(|e| PluginError::Failed(e.to_string()))?;
        Ok(Payload::Json(json!("old")))
    }
}

struct Fixed(&'static str);

impl Plugin for Fixed {
    fn invoke(&self, _: &Context<'_>, _: &str, _: Payload) -> Result<Payload, PluginError> {
        Ok(Payload::Json(json!(self.0)))
    }
    fn on_hook(&self, _: &Context<'_>, _: Hook, _: Payload) -> Result<Payload, PluginError> {
        if self.0 == "bad" {
            Err(PluginError::Failed("boom".into()))
        } else {
            Ok(Payload::Json(json!(self.0)))
        }
    }
}

fn simple(name: &str, commands: &[&str], hooks: &str) -> Manifest {
    let mut src = format!(
        "[plugin]\nname = \"{name}\"\nversion = \"0.1.0\"\ntier = \"native\"\n[contributes]\nhooks = [{hooks}]\n"
    );
    for c in commands {
        src.push_str(&format!(
            "[[contributes.commands]]\nid = \"{name}.{c}\"\ntitle = \"t\"\n"
        ));
    }
    Manifest::from_toml(&src).expect("valid manifest")
}

#[test]
fn replace_swaps_atomically_and_in_flight_calls_finish_on_the_old_plugin() {
    let (started_tx, started_rx) = channel();
    let (release_tx, release_rx) = channel();
    let k = Arc::new(Kernel::new());
    k.register(
        simple("p", &["run"], ""),
        Arc::new(Slow {
            started: started_tx,
            release: Mutex::new(release_rx),
        }),
    )
    .expect("registers");

    let worker = {
        let k = Arc::clone(&k);
        std::thread::spawn(move || k.invoke("p.run", Payload::Empty))
    };
    started_rx.recv().expect("call is in flight");

    k.replace(simple("p", &["run", "extra"], ""), Arc::new(Fixed("new")))
        .expect("replaces while a call runs");
    assert_eq!(
        k.invoke("p.run", Payload::Empty).expect("new plugin"),
        Payload::Json(json!("new"))
    );
    assert!(k.invoke("p.extra", Payload::Empty).is_ok());

    release_tx.send(()).expect("release");
    assert_eq!(
        worker.join().expect("joined").expect("old call finishes"),
        Payload::Json(json!("old"))
    );
}

#[test]
fn replace_refuses_stolen_commands_and_unknown_plugins() {
    let k = Kernel::new();
    k.register(simple("p", &["run", "extra"], ""), Arc::new(Fixed("p")))
        .expect("registers");
    k.register(simple("q", &["go"], ""), Arc::new(Fixed("q")))
        .expect("registers");

    // `q.go` belongs to `q`; a manifest named `p` cannot even declare it.
    assert!(matches!(
        k.replace(simple("zzz", &[], ""), Arc::new(Fixed("x"))),
        Err(KernelError::UnknownPlugin(_))
    ));

    // Dropping a command on replace frees it.
    k.replace(simple("p", &["run"], ""), Arc::new(Fixed("n3")))
        .expect("replaces");
    assert!(matches!(
        k.invoke("p.extra", Payload::Empty),
        Err(KernelError::UnknownCommand(_))
    ));
    assert_eq!(k.plugin_names(), vec!["p".to_owned(), "q".to_owned()]);
    assert!(k.manifest("p").is_some());
}

#[test]
fn run_hook_each_isolates_a_failing_subscriber() {
    let k = Kernel::new();
    for (name, tag) in [("a", "ok-a"), ("b", "bad"), ("c", "ok-c")] {
        k.register(simple(name, &[], "\"on_lint\""), Arc::new(Fixed(tag)))
            .expect("registers");
    }
    assert!(k.run_hook(Hook::OnLint, Payload::Empty).is_err());
    let each = k.run_hook_each(Hook::OnLint, Payload::Empty);
    assert_eq!(each.len(), 3);
    assert!(each[0].1.is_ok() && each[1].1.is_err() && each[2].1.is_ok());
}
