# Writing Manang plugins

Every Manang feature is a plugin. First-party plugins are native Rust compiled
into the app. Third-party plugins are **WebAssembly components** that run in a
Wasmtime sandbox with no ambient access to your machine: they get exactly the
host services their manifest asks for, and nothing else. This guide covers the
WASM tier. Design background is in
[ADR 0003](adr/0003-wasm-plugin-runtime.md).

## A plugin in thirty seconds

```
my-plugin/
  manifest.toml     identity, permissions, commands, hooks
  plugin.wasm       the compiled component
```

Install it by copying the directory to `<config>/manang/plugins/my-plugin/`
(on Linux, `~/.config/manang/plugins/`). Manang loads it at start-up and
hot-reloads it whenever the files change. The directory name must equal the
plugin name.

Start from the template:

```sh
rustup target add wasm32-unknown-unknown
cp -r sdk/rust-plugin-template ~/src/my-plugin     # edit name, manifest, src/lib.rs
scripts/build-plugin.sh ~/src/my-plugin            # -> ~/src/my-plugin/dist/my-plugin/
cp -r ~/src/my-plugin/dist/my-plugin ~/.config/manang/plugins/
```

`plugins/word-count` is a complete worked example: a command
(`word-count.count`), an `on_lint` hook, and a `buffer` permission.

## The stateless-per-call model

The host builds a **fresh instance for every command and every hook call** and
throws it away afterwards. Consequences:

- Nothing in `static`s, globals or linear memory survives between calls. Do not
  cache in the plugin.
- Calls are isolated from each other and can run in parallel.
- A crash, trap, timeout or memory blow-up in one call cannot poison the next.
- Anything durable goes through host services: write a file (`fs`), edit the
  buffer (`buffer`), or return it to the caller.
- Start-up cost counts on every call. Keep initialisation trivial. The host's
  overhead for an empty call is about 30 to 45 microseconds; your code is the
  rest.

## manifest.toml

```toml
[plugin]
name = "word-count"            # lowercase kebab-case, starts with a letter
version = "0.1.0"              # semver
description = "One line."      # optional
# entry = "plugin.wasm"        # optional; relative path inside the directory
# tier = "wasm"                # the default; "native" is first-party only

[permissions]                  # everything denied unless listed
buffer = ["read:active"]
filesystem = ["read:templates", "write:out"]
network = ["api.crossref.org"]

[contributes]
hooks = ["on_lint"]

[[contributes.commands]]
id = "word-count.count"        # must be "<plugin name>.<command>"
title = "Count words"          # shown in the command palette
```

Unknown keys under `[permissions]` and malformed entries are errors, not
warnings. A manifest larger than 64 KiB, or with more than 256 commands or
permission entries, is refused.

### Permissions

| Key          | Entry form          | Meaning |
| :----------- | :------------------ | :------ |
| `buffer`     | `read:active`, `write:active` | Read or replace the active document. `write` implies `read`. |
| `filesystem` | `read:<dir>`, `write:<dir>`   | Files under `<project>/<dir>`. `write` implies `read`. `<dir>` is relative, with no `..`. |
| `network`    | `<host>`            | Exact host name, case-insensitive. No wildcards, no schemes, no ports. |

A permission also decides which interfaces the host links. A component that
imports `fs` without any `filesystem` permission is **rejected at load** with
`UngrantedImport`, rather than failing at call time.

### Hooks

| Hook                  | Mode     | Notes |
| :-------------------- | :------- | :---- |
| `on_compile_pre`      | pipeline | Each subscriber's output feeds the next, in directory-name order. |
| `on_compile_post`     | pipeline | As above. |
| `on_lint`             | fan-out  | Every subscriber sees the same input; one failing plugin does not hide the others' results. |
| `register_agent_tool` | fan-out  | |

Hook and command payloads are JSON for small structured data, or raw bytes for
artefacts (PDF, SVG). Payload shapes per hook are owned by the plugin that
fires the hook; `word-count` documents its `on_lint` contract in its source.

## The WIT world

Plugins speak the `manang:plugin@0.2.0` WIT package
(`crates/manang-wasm-host/wit/plugin.wit`). Pick the world that matches your
permissions:

| World           | Imports                       | Use when |
| :-------------- | :---------------------------- | :------- |
| `plugin`        | none                          | pure computation |
| `plugin-buffer` | `buffer`                      | you edit or read the open document |
| `plugin-fs`     | `fs`                          | you read or write project files |
| `plugin-http`   | `http`                        | you call web APIs |
| `plugin-full`   | `buffer`, `fs`, `http`        | several of the above; only interfaces you actually call are imported |

Every world exports `handler`:

```wit
invoke:  func(command: string, input: payload) -> result<payload, failure>;
on-hook: func(hook: hook, input: payload) -> result<payload, failure>;
```

`payload` is `empty | json(string) | binary(list<u8>)`. Return `Err(failure)` to
report a user-visible error. There is no WASI: no clock, no environment, no
random source, no sockets. Plugins are deterministic unless a host service says
otherwise.

### Host interfaces

- **`buffer`**: `read-active() -> string`, `write-active(text)`.
- **`fs`**: `read-file(path)`, `write-file(path, data)`, `list-dir(path)`.
  Paths are relative to the open project, `/`-separated. Absolute paths, `..`,
  empty segments, backslashes, `:` and NUL are refused. A path must fall inside
  a granted scope, and the host opens it *beneath* the scope directory, so a
  symlink pointing out of the scope is refused even if it is created while the
  call runs. `write-file` creates parent directories inside the scope and
  replaces existing files. No project open means every `fs` call fails.
- **`http`**: `fetch(request) -> response`. Methods GET, HEAD, POST, PUT,
  DELETE; `http` and `https` only; no credentials in URLs; the host header and
  framing headers are set by the host. The host of the request and of **every
  redirect hop** must be in `network`; ambient proxy settings are ignored.

## Limits

Defaults (`Limits::default()`); every one is a hard cap enforced by the host.

| Limit | Default | On breach |
| :---- | :------ | :-------- |
| Linear memory per call | 64 MiB | trap; call fails |
| Wall-clock for guest code per call, instantiation included | 250 ms | trap |
| Host I/O time per call (`fs`, `http`), not charged to the line above | 30 s | further I/O refused |
| One `http` request, redirects included | 10 s | request fails |
| Payload into the plugin | 16 MiB | call refused |
| Payload out of the plugin | 16 MiB | call fails |
| Table elements | 100 000 | trap |
| Core instances / memories / tables per component | 16 / 4 / 8 | load or call fails |
| One `fs` file | 16 MiB | refused |
| Total `fs` traffic per call | 64 MiB | refused |
| `http` request or response body | 8 MiB | refused |
| Error text returned to the app | 4 KiB | truncated |
| Plugin `.wasm` / manifest size | 64 MiB / 64 KiB | not loaded |

Design for the 250 ms budget: do heavy work in a first-party plugin, or split
it into several commands.

## Building

`scripts/build-plugin.sh <crate-dir> [out-dir]` builds the crate for
`wasm32-unknown-unknown`, encodes the core module as a component with
`sdk/componentize` (a thin wrapper over `wit-component`), and writes
`<out-dir>/<name>/{manifest.toml,plugin.wasm}`. Any language with a component
toolchain works; the contract is only "a component of one of the worlds above".

Keep your crate's `wit/plugin.wit` in sync with the host's (copy it from
`crates/manang-wasm-host/wit/`). A component built against another version of
the package is rejected with `IncompatibleApi`; rebuild it.

## Installing, reloading, removing

- Put the directory under `<config>/manang/plugins/`. At start-up Manang loads
  every plugin; a broken one is reported in the plugin list and does not stop
  the others.
- While Manang runs, file changes are debounced (about 250 ms) and the plugin
  is swapped atomically. A call already running finishes on the old version; new
  calls use the new one. If the new version fails to load, the old one keeps
  running and the error is reported.
- Delete the directory to unload the plugin.

## Cold start and integrity

Compiled machine code is cached in `<cache>/manang/wasm` as `.cwasm`, keyed by
the BLAKE3 hash of your `.wasm` plus the engine configuration. Entries carry a keyed
BLAKE3 MAC under a per-installation secret, and the host refuses to load any
entry that fails verification, because deserialising machine code is `unsafe`.
A tampered or stale entry is recompiled and replaced. A cache hit loads a
plugin in about 0.2 ms instead of about 40 ms.

The cache protects against corruption and against writers that cannot read the
key. It is not a substitute for checking where a plugin came from. Plugin
signing (publisher keys, an ed25519 signature over manifest and wasm, verified
before load) is not implemented yet; see the ADR follow-ups.

## Testing your plugin

Drive it through the kernel exactly as the app does: build it, install it in a
temporary plugins directory, create a `PluginManager`, `scan()`, then
`kernel.invoke("my-plugin.hello", Payload::Json(...))`. The test in
`crates/manang-plugins/tests/sdk.rs` is a working pattern, including providing
a fake `BufferService`.
