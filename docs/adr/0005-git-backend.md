# ADR 0005: Git backend: gix for reads, the system `git` binary for writes

Status: accepted, 2026-10-17

## Context

The git plugin (`crates/manang-git`) needs status, diff, log, blame, show and
branch listing (read path), and stage, unstage, discard, commit, branch switch,
init, and fetch/pull/push (write path). AGENTS.md prefers Rust-first and pure
Rust where robust. Three candidates:

- **gix (gitoxide)**, 0.89.0 on crates.io (the version `manang-project` already
  pins), pure Rust, no C toolchain.
- **git2 (libgit2)**, 0.21.0, mature C library with Rust bindings. Needs a C
  build (or system libgit2) and OpenSSL/libssh2 for network work. Its API is the
  most complete for local writes, but libgit2 does not run hooks, ignores some
  newer config (e.g. `includeIf` corner cases, `core.fsmonitor`) and lags
  upstream git on index/filter behaviour.
- **The system `git` executable**, driven as a subprocess (no shell).

Research (gitoxide `crate-status.md`, release notes through the August 2026
0.56 CLI release, and the crate docs for gix 0.89) shows the current state:

| Operation | gix status (Oct 2026) |
| --- | --- |
| Open/discover, config, refs, rev-parse, rev-walk | mature, stable plumbing |
| Object reads, tree lookup by path | mature |
| Status (index vs work tree, tree vs index, untracked) | works well; rename tracking has gaps |
| Tree/blob diff, line diff (imara-diff) | works; rename/copy deviations from git |
| Blame (`gix-blame`) | works for single files; no work-tree blame, partial rename tracking |
| Index write, ref transactions, commit object creation | plumbing exists |
| Checkout / switch / restore / reset | only low-level `gix-worktree-state`; no porcelain; safe "refuse if dirty" logic not provided |
| Merge | blob and commit merge exist; no merge orchestration, tree-merge gaps |
| Stash | not usable (planned) |
| Push | protocol pieces only; no high-level push |
| Fetch/clone | mostly working, but credential/SSH behaviour differs from the user's git |
| Hooks, gpg, attribute/CRLF filters on write | not provided |

So gix is a good read engine and not yet a safe write engine.

## Decision

Per operation:

| Operation | Backend | Why |
| --- | --- | --- |
| `status` (files) | gix via `manang_project::git::status` (reused) | already shipped; pure Rust |
| `status` upstream ahead/behind | gix rev-walk with hidden tips | no subprocess on the hot path |
| `diff`, `diff-text` | gix reads HEAD/index blobs, `similar` computes hunks | structured output with UTF-16 offsets is ours to define; `similar` is tiny and pure Rust |
| `log`, `blame`, `branches`, `show` | gix | mature enough; binary-safe blob reads |
| config identity check | gix config snapshot | same files git reads |
| `stage`, `unstage`, `discard` | system git (`add`, `reset`/`rm --cached`, `clean` + `restore`) | index writing must honour `.gitattributes` filters, CRLF rules, sparse checkout and file locking exactly as the user's git does; gix has no equivalent of `git add` semantics |
| `commit` | system git | hooks (opt-in), identity rules, `--amend`, message cleanup, reflog |
| `checkout-branch` | system git `switch` | gix has no checkout porcelain; git's own refusal to overwrite local changes is the safety we want |
| `init` | system git | one call, honours `init.defaultBranch` and templates |
| `sync` (fetch/pull/push) | system git | credential helpers and ssh-agent are the user's; gix push is incomplete |

libgit2 was rejected: it adds a C dependency and a second index implementation
that can disagree with the user's git (filters, hooks), and the system binary
gives the same write semantics for free. The cost is that the git executable must
be installed; its absence is a structured `git-missing` error and reads keep
working.

Gaps accepted on the gix read path: staged renames are shown as an addition of
the new path (the old path is not listed); line diffs compare raw bytes, so a
repository using `core.autocrlf` or `text` attributes can show whole-file diffs
for files whose work-tree line endings differ from the stored blob; blame covers
committed content only; `log --path` uses "differs from all parents"
simplification without rename following.

## Subprocess safety

All writes go through one runner (`runner.rs`):

- program `git`, argument array, never a shell; paths follow `--`, with
  `--literal-pathspecs` so `*` and `:(glob)` are plain characters;
- environment cleared and rebuilt from an allowlist (`PATH`, `HOME`, config
  locations, `SSH_AUTH_SOCK`, a few `GIT_*` for config/CA paths). Ambient
  `GIT_DIR`, `GIT_SSH_COMMAND`, `GIT_CONFIG_PARAMETERS` and similar are dropped;
- `core.hooksPath` points at the null device unless the call passes
  `hooks: true`; `commit.gpgsign=false`; `protocol.ext.allow=never`;
- stdin closed, `GIT_TERMINAL_PROMPT=0`, `GIT_EDITOR=true`, `LC_ALL=C`; for
  `sync` ssh runs with `BatchMode=yes` unless the user configured
  `core.sshCommand`;
- hard timeout (30 s local, 120 s default and 600 s maximum for `sync`) that kills
  the child; stdout capped at 8 MiB, stderr at 64 KiB;
- every caller path is validated with `manang_project::ProjectRoot`
  (lexical, symlink and `.git` guards) before it reaches an argument.

Policy: no force push (explicit `refs/heads/b:refs/heads/b` refspec, no `+`),
pull is `--ff-only`, remotes must be names already configured (no URLs, no
`ext::`), `--amend` only when no remote-tracking ref contains `HEAD`, discard
needs `confirm: true` and first copies every affected file to
`<git-dir>/manang-trash/<timestamp>/`. The network is touched only by
`git.sync`.

## Consequences

- Revisit when gix ships checkout/restore porcelain and push: stage and
  switch are the first candidates to move, behind the same command surface.
- Known limitation: ssh may still read a controlling terminal if the host app
  has one; the timeout bounds the damage.
- Read paths construct a `gix::Repository` per call (single-digit milliseconds);
  see the criterion bench `status_diff` for the 20k-file numbers.
