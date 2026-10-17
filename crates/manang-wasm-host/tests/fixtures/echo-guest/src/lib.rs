//! Test plugin. Exercises payloads, the buffer import, and the sandbox limits.

wit_bindgen::generate!({
    path: "../../../wit",
    world: "plugin-buffer",
});

use exports::manang::plugin::handler::Guest;
use manang::plugin::buffer;
use manang::plugin::types::{Failure, Hook, Payload};

struct Echo;

fn fail(message: &str) -> Failure {
    Failure {
        message: message.to_owned(),
    }
}

#[inline(never)]
#[allow(unconditional_recursion)] // intentional: the host must survive it
fn recurse(n: u64) -> u64 {
    let big = [n as u8; 256];
    std::hint::black_box(big);
    recurse(n + 1) + u64::from(big[0])
}

impl Guest for Echo {
    fn invoke(command: String, input: Payload) -> Result<Payload, Failure> {
        match command.as_str() {
            "echo.say" => Ok(input),
            "echo.upper" => match input {
                Payload::Json(s) => Ok(Payload::Json(s.to_uppercase())),
                _ => Err(fail("expected json")),
            },
            "echo.bytes" => match input {
                Payload::Binary(b) => Ok(Payload::Binary(b.into_iter().rev().collect())),
                _ => Err(fail("expected binary")),
            },
            "echo.read" => buffer::read_active().map(Payload::Json),
            "echo.write" => match input {
                Payload::Json(s) => buffer::write_active(&s).map(|()| Payload::Empty),
                _ => Err(fail("expected json")),
            },
            // Never returns: the host's epoch deadline must stop it.
            "echo.spin" => loop {
                std::hint::black_box(0u64);
            },
            // Tries to allocate far more than the host's memory cap.
            "echo.grow" => {
                let mut v: Vec<u8> = Vec::new();
                v.resize(512 * 1024 * 1024, 1);
                Ok(Payload::Json(std::hint::black_box(v).len().to_string()))
            }
            // Returns `n` zero bytes, where `n` is the JSON input.
            "echo.big" => match input {
                Payload::Json(n) => {
                    let n: usize = n.parse().map_err(|_| fail("expected a number"))?;
                    Ok(Payload::Binary(vec![0; n]))
                }
                _ => Err(fail("expected json")),
            },
            "echo.bigjson" => match input {
                Payload::Json(n) => {
                    let n: usize = n.parse().map_err(|_| fail("expected a number"))?;
                    Ok(Payload::Json(format!("\"{}\"", "x".repeat(n))))
                }
                _ => Err(fail("expected json")),
            },
            "echo.badjson" => Ok(Payload::Json("{not json".to_owned())),
            "echo.longerr" => Err(fail(&"e".repeat(100_000))),
            "echo.trap" => panic!("guest panics on purpose"),
            // Unbounded recursion: must hit the stack limit, not crash the host.
            "echo.stack" => Ok(Payload::Json(recurse(1).to_string())),
            other => Err(fail(&format!("unknown command {other}"))),
        }
    }

    fn on_hook(_hook: Hook, input: Payload) -> Result<Payload, Failure> {
        Ok(input)
    }
}

export!(Echo);
