# manang-git: wiring notes

## src-tauri

1. `Cargo.toml`: `manang-git = { path = "../crates/manang-git" }`.
2. Register next to the project plugin:
   `kernel.register(manang_git::manifest()?, Arc::new(manang_git::plugin(Some(bus.clone()))))?;`
3. The git plugin does not see the project plugin's state. After `project.open`
   / `project.create` the shell must call `git.open {root}` (or pass `root` on
   each command; the last root is remembered). `root` is an absolute directory.
4. Forward the `git.changed` event topic (payload `{root}`) to the frontend the
   same way `project.changed` is forwarded.
5. Run commands from a blocking task: they are synchronous and spawn `git`.
6. Errors: `PluginError::Failed(s)` where `s` is the JSON text
   `{"code": "...", "message": "..."}`. Codes: no-root, not-a-repo,
   already-a-repo, bad-path, bad-arguments, confirm-required, missing-identity,
   nothing-to-commit, empty-message, amend-published, amend-unborn, dirty-tree,
   bad-branch-name, git-missing, timeout, too-large, command-failed,
   rev-not-found, path-not-in-rev, backend, io.
7. `git.show` returns `Payload::Binary` for non-UTF-8 content and JSON
   `{rev, path, text}` otherwise.
8. Permissions: manifest declares `filesystem = ["write:project"]` only. The
   kernel's network capability model wants bare host names, which a user's
   remotes cannot be, so `git.sync` is gated by being an explicit command and
   is not declared as `network`.

## Commands (all accept optional `root`)

| command | args | result |
| --- | --- | --- |
| git.open | `{root}` | `{root, is_repo}` |
| git.status | `{}` | `{branch, detached, unborn, head, upstream:{name,ahead,behind}\|null, files:[{path, staged, unstaged, untracked, conflicted}], truncated}` |
| git.diff | `{path?, staged?}` | `{staged, files:[{path,status,binary,too_large,additions,deletions,hunks:[{old_start,old_lines,new_start,new_lines,header,lines:[{kind,text,old_line,new_line,old_offset,new_offset,no_newline}]}]}]}`; offsets are UTF-16 units |
| git.diff-text | `{path, staged?}` | `{text}` (`path: "."` for everything) |
| git.log | `{path?, limit?, offset?}` | `{commits:[{hash,short,author_name,author_email,time,subject,parents}], has_more}` |
| git.blame | `{path}` | `{path,total_lines,ranges:[{start,lines,commit,source_path}]}` (HEAD content) |
| git.branches | `{}` | `[{name,remote,current,hash,upstream}]` |
| git.show | `{rev, path}` | JSON `{rev,path,text}` or Binary |
| git.stage / git.unstage | `{paths}` (`"."` = whole project) | `{staged}` / `{unstaged}` |
| git.discard | `{paths, confirm: true}` | `{discarded, trash}` |
| git.commit | `{message, amend?, hooks?}` | `{hash,short,subject,amended}` |
| git.checkout-branch | `{name, create?}` | `{branch, created}` |
| git.init | `{}` | status |
| git.sync | `{action: fetch\|pull\|push, remote?, timeout_secs?, hooks?}` | `{action, remote, output}` |

## Requests for other crates

None required. (`manang-project` could expose its `.manang` trash for discard
copies; discard uses `<git-dir>/manang-trash` instead so the work tree stays
clean.)
