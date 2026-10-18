# Persistent formats, versions and safety nets

Every file Manang reads or writes, its current version, and what happens when
an update meets yesterday's file (or yesterday's Manang meets tomorrow's).

## Policy

1. **A version key.** Files carry an integer `version` (figure scenes: `version`
   too). A file without one predates the key and is **version 1**.
2. **Older: migrate forward, with a backup.** Migrations only go forward. Before
   a migrated file replaces the old one, the old file is copied once to
   `<name>.bak-v<N>` next to it (never replaced by a later backup).
3. **Newer: read-only.** A file with a higher version than this build knows is
   read as far as it is understood and **never written back** (so a downgrade
   cannot destroy what a later release stored). The user is told. Figure scenes
   from the future are refused at open, which has the same effect.
4. **Damaged: set aside, never blocking.** Machine-written files that cannot be
   parsed are renamed to `<name>.corrupt` (`.corrupt.1`, ... if one exists) with
   a warning, and a fresh file is used. Hand-edited files (`settings.toml`,
   `keybindings.toml`, `.manang/settings.toml`) are **not** moved, because the
   person may be mid-edit: they are left exactly as they are, never rewritten
   while invalid, the app runs on defaults and says why.
5. **Never delete user data.** Helpers live in `crates/manang-formats` (read a
   version, migrate chain, backup once, set aside, atomic write), one tiny leaf
   crate so every format crate shares one implementation.

Golden fixtures for every shipped version are in `testdata/formats/<format>/v1/`
(the file as the first release wrote it) plus a made-up `v999/` that must stay
read-only and lose nothing. Tests: `crates/*/tests/formats.rs`.

## Inventory

| Format | Where | Version | Older | Newer | Damaged |
| :-- | :-- | :-- | :-- | :-- | :-- |
| `manang.toml` | project root | 1 (`version`, optional) | chain ready (no older exists) | opens, unknown keys ignored, never rewritten | error listing every problem (user-owned file, not moved) |
| `settings.toml` | user config dir | 1 (`version`, stamped on first write) | chain ready, `.bak-vN` | read, writes refused | left as is, warning, defaults |
| `.manang/settings.toml` | project | 1 | as above | as above | as above |
| `keybindings.toml` | user config dir | 1 | as above | as above | previous bindings kept, warning |
| `themes/*.toml` | user config dir | `version` accepted, unknown keys rejected per file | n/a | file rejected with a warning | warning, others load |
| `plugins.toml` | `<config>/manang/` | 1 | chain ready | read, writes refused, warning | `.corrupt`, reset, warning |
| `.manang/plugins.toml` | project | 1 (read-only for Manang) | n/a | read leniently | error |
| `projects.toml` | user config dir | 1 | chain ready, `.bak-vN` | listed, writes refused, warning | `.corrupt`, empty list, warning |
| `recent-projects.toml` | user config dir | 1 | as above | as above | as above |
| `*.figure` / `*.figure.json` | project | 2 (v1 -> v2 migration shipped) | migrated on open; `.bak-v1` kept on first save | refused at open | error, file untouched |
| recovery snapshots | `<app data>/recovery/<project>/<file>.json` | 1 | chain ready | not offered, not deleted | `.corrupt`, others still listed |
| `.manang/cache`, thumbnails | project / user cache | by content key | disposable: rebuilt | rebuilt | rebuilt |
| `.manang/trash/` | project | folders `<id>/<original path>` | n/a | n/a | never touched automatically |
| localStorage | browser/webview | key suffix (`manang.layout.v1`, `manang.session.v1`, `manang.sessions.v1`, `manang.home.v1`; `manang.figure.*` unversioned) | an unknown shape reads as defaults | ignored | ignored |

Notes. `version` in settings files is the file's, not a setting (it is not
reported as an unknown setting). Figure scenes written at v2 before real math
rendering (`tex`) are upgraded in memory and written in the new shape without a
backup, since the version number did not change. localStorage holds only
disposable view state (layout, tabs, sort); nothing there is user work.

## Safety nets

| Operation | Safety net | Where the copy goes | How to recover |
| :-- | :-- | :-- | :-- |
| Unsaved edits, crash, force-quit | recovery snapshot, 2 s after the last edit (at most every 10 s), also before the close dialog | `<app data>/recovery/<project>/` | the recovery dialog on next open (Recover all / Review / Discard) |
| File changed outside while open | clean: silent reload; dirty: bar, nothing overwritten; save checks the loaded fingerprint (`[conflict]`) | n/a | Keep my version / Load disk version (Ctrl+Z restores yours) / Compare |
| Overwrite with my version, Keep my version | the disk version is copied first | `.manang/trash/<id>/<path>` | copy it back |
| Convert to Typst over an existing file | asks, then copies the old file | `.manang/trash/<id>/<path>` | copy it back |
| Project-wide replace | every changed file's original saved first (fails closed) | `.manang/trash/<id>/` (reported in the notice) | copy files back |
| Delete a file | moved, not removed; unsaved edits saved into it first | `.manang/trash/<id>/<path>` | move it back |
| Rename / move | refuses to overwrite an existing file | n/a | n/a |
| Save as | refuses to overwrite (`absent`) | n/a | n/a |
| File deleted outside | tab and text kept | n/a | Save again / Save as |
| Discard changes (Source control) | copies first | `<git-dir>/manang-trash/<stamp>/` | copy back |
| Restore from Versions | current changes saved as a version first; copies of replaced files | `<git-dir>/manang-trash/` | restore the "Before restoring" version |
| Plugin uninstall / update | moved aside | `<config>/manang/plugins-trash/` | move back |
| Project trash purge | removes the list entry only; "delete files" goes to the OS trash | OS trash | restore from the OS trash |
| Migrated file replaced (settings, lists, figures) | `<name>.bak-v<N>` | next to the file | rename it back |
