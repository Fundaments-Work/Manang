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
   rev-not-found, path-not-in-rev, nothing-to-save, backend, io.
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
| git.commit | `{message, amend?, hooks?, trailers?: ["Key: value"], all?}` (`all` stages every non-ignored change first) | `{hash,short,subject,amended}` |
| git.checkout-branch | `{name, create?}` | `{branch, created}` |
| git.restore | `{rev, paths?, whole?, save_first?}` | `{restored, untouched_new, safety_version, trash}`; never deletes or rewrites history; error `dirty-tree` carries `paths` unless `save_first` |
| git.identity-get | `{}` | `{name, email, source: repo\|global\|none}` |
| git.identity-set | `{name, email, global?}` | `{ok}` (repo-local by default) |
| git.save-version | `{name?, message?}` | `{sha, short, subject, files_changed}`; errors `nothing-to-save`, `missing-identity`; commit carries trailer `Manang-Version: 1` |
| git.versions | `{path?, limit?, cursor?}` | `{versions:[{hash,short,subject,author_name,time,files_changed,labelled}], next_cursor}` |
| git.version-files | `{rev}` | `{rev, files:[FileDiff without hunks]}` |
| git.version-diff | `{rev, path}` | one `FileDiff` (against the first parent) |
| git.init | `{}` | status |
| git.sync | `{action: fetch\|pull\|push, remote?, timeout_secs?, hooks?}` | `{action, remote, output}` |

## Requests for other crates

None required. (`manang-project` could expose its `.manang` trash for discard
copies; discard uses `<git-dir>/manang-trash` instead so the work tree stays
clean.)

## Share and get updates (`remote.rs`)

| command | args | result |
| --- | --- | --- |
| git.remote-get | `{}` | `{name, url}` (null without a remote) |
| git.remote-set | `{url}` | `{name, url}`; adds origin or updates the existing remote; refuses `-`, `ext:`, whitespace |
| git.sync-status | `{refresh?}` | `{state: no-remote/up-to-date/ahead/behind/diverged, remote, branch, ahead, behind, never_shared, uncommitted}`; local data only unless `refresh` |
| git.share | `{}` | `{remote, branch, pushed}`; plain push with upstream, never forced |
| git.get-updates | `{strategy?: merge (default) or ff-only}` | `{result: up-to-date/fast-forwarded/merged/diverged/conflicts, files, received, safety_version}` |

`conflicts` and `diverged` are results, not errors: HEAD and the work tree are unchanged (detected with `git merge-tree --write-tree`, git 2.38+). With `merge`, unsaved work is saved as a version first. Error codes: no-remote, bad-remote-url, nothing-to-share, share-rejected, auth-failed, remote-unreachable, unrelated-histories. Credentials come from the system git; the runner never prompts.
