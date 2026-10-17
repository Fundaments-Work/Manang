# manang-writing: notes for other crates

Changes wanted outside this crate (not made here, by agreement).

## Wiring in `src-tauri`

```rust
let config_dir = app.path().app_config_dir()?;           // injected, created on first write
kernel.register(manang_writing::manifest()?, Arc::new(manang_writing::plugin(config_dir)))?;
```

* Files the plugin uses inside `config_dir`: `user-dictionary.txt`,
  `snippets.toml`, `dictionaries/`.
* `invoke` is CPU-bound and blocks the calling thread (it runs on a private
  2-thread Rayon pool). Dispatch from async code through `spawn_blocking`.
* `on_lint` input: `{text|source, syntax?, rules?, file?, spell?, language?}`.
  Output: `{source: "writing", diagnostics: [...]}`, same envelope as the Typst
  plugin. Pass `file` so diagnostics carry the document path, and `syntax`
  (`typst|latex|markdown`) so masking is exact; without it the syntax is
  auto-detected.
* Offsets everywhere are Unicode scalar (character) offsets, like
  `manang-typst`.

## Diagnostic shape

Identical to `manang-typst` (`severity, message, file, line, column, range,
hints, trace`) plus an extra `code` field (the rule id, e.g. `repeated-word`,
`spelling`). One extension: `severity` may be `info` (style advice) in addition
to `error`/`warning`. The frontend diagnostics mapper should treat `info` as
the lowest severity (CodeMirror `info`).

## Third-party notices to show in the UI

* typstyle-core (Apache-2.0), spellbook (MPL-2.0), pulldown-cmark (MIT).
* SCOWL dictionary licence: `crates/manang-writing/dict/LICENSE-SCOWL-en_US.txt`
  must appear in the licences screen (see ADR 0010).

## Toolchain

`typstyle-core` 0.15 needs Rust 1.92; the crate sets `rust-version = "1.92"`.
The workspace `rust-version = "1.85"` is lower than what the Typst 0.15
dependency tree already requires; consider raising it.

## Not done

* LaTeX formatting returns `unsupported` by design (ADR 0010).
* Dictionaries other than en_US / en_GB are loaded from `<config_dir>/dictionaries/`.
* The user dictionary is loaded once; edits made to the file outside the app
  are picked up on restart (snippets are hot-reloaded).
