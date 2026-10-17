# manang-search notes

## Wiring (src-tauri)

```rust
let manifest = manang_search::manifest()?;
kernel.register(manifest, Arc::new(manang_search::plugin(Some(bus.clone()))))?;
```

* Commands are synchronous: invoke from a blocking task (`spawn_blocking`),
  never on the UI thread or a Tokio worker. `search.cancel` must be invoked
  concurrently (a second blocking task), since the running `search.text`
  occupies its caller until it returns.
* Pass the same `EventBus` the project plugin publishes on; the plugin
  subscribes to `project.changed` and drops stale file indexes. With `None`
  the index expires after 2 s instead.
* Newer `search.text` / `search.replace-preview` calls cancel older ones
  automatically; the front end only needs `search.cancel` for the "stop"
  button (omit `id` to cancel everything in flight).

## Changes wanted elsewhere (worked around)

* `manang_project::paths::STATE_DIR` is not re-exported; `.manang` is
  hard-coded in `index.rs`. Re-export it from manang-project to share it.
* `manang_project::tree` has no reusable "walk all files" API; the index
  re-implements the `WalkBuilder` setup (same options: no git_global,
  require_git(false), no symlink following, `.git`/root `.manang` excluded).

## Known limits

* Results are returned in one response (no streaming of partial hits).
* In-document find uses `$`/`^` at `\n` only (no CRLF) because grep-regex
  forbids `\n` in patterns when CRLF mode is on; the editor rope is LF.
* Replace skips files that are not valid UTF-8 or contain NUL.
* Check-then-use race on apply (hash check, then atomic rename) is the same
  accepted limitation as manang-project's path confinement.
