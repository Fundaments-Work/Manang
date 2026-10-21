# Use Manang with Git

Manang has a Source control panel for Git commits and branches, plus plain-language versions and sharing; this page explains what each does and where it stops.

Manang uses Git in two ways that share one repository. The **Changes** view is a conventional Git panel: stage files, write commit messages, switch branches. The **Versions** view is a simplified layer on top: named snapshots, a timeline, restore, **Share** and **Get updates**. You can use either, or both on the same project, and any other Git tool sees the same history. This page covers how they relate and what is not supported.

## What does Manang need to work with Git?

- **A `git` program on your computer.** Manang reads history itself, but every write (stage, commit, switch branch, share, get updates) runs your installed `git`, so it uses your Git configuration, credential helper and ssh keys. Without Git installed, reading history still works and writes report that Git is missing; the design reasoning is in `docs/adr/0005-git-backend.md` in the source repository.
- **Your name and email.** Commits and versions carry an author. If Git does not know who you are, Manang asks once and offers to save the answer for this project or for all projects.
- **A repository.** If the folder is not yet one, the Changes view shows **Initialize repository** and the Versions view offers **Start keeping versions**. Neither changes your files.

## How do I commit changes?

1. Click **Source control** in the Activity bar. The panel opens on **Changes**.
2. If the project is not a repository, click **Initialize repository**.
3. Edit your documents. Changed files appear under **Changes**, files already staged under **Staged Changes**, and new files under **Untracked**. Click a file to see its diff with added and removed lines.
4. Click **+** beside a file, or **Stage all** on a section header, to stage. Click **-** to unstage. Staging is per file.
5. Type a summary in the message box. Optionally tick **Amend** to replace the most recent commit.
6. Click **Commit**, or press `Ctrl+Enter` (`Cmd+Enter`) in the message box. A notice shows the short identifier and the subject.

The trash icon beside an unstaged file discards its changes after a confirmation. Before discarding, Manang copies each affected file into `.git/manang-trash/` in the project, so a mistaken discard is recoverable by hand. See [Crash Recovery and Safety Copies](crash-recovery.md#source-control-discard-and-checkout).

Amend is refused when the last commit is already on a remote, because that would rewrite published history. Git hooks do not run, and commits are not GPG-signed.

## How do I work with branches?

1. Click the branch name at the top of the panel. A list shows the local branches, with the current one marked.
2. Click a branch to switch to it. Git refuses the switch if it would overwrite uncommitted work; commit, stage or discard first, then try again.
3. To start a new branch from where you are, click **+ Create new branch…**, type a name and click **Create**. Manang switches to it.

Branches are useful for a rewritten chapter or a reviewer's round of comments: work on `chapter-3-rewrite`, and return to the main branch if you do not like it. Files on disk change when you switch, and open tabs follow. If an open file with unsaved edits changes on disk, a bar asks whether to keep your version or load the disk version.

There is no merge, rebase, delete-branch or tag button in the panel. Do those with the `git` command line or another Git tool; Manang shows the result.

## What is the difference between a version and a Git commit?

A saved version is a Git commit with a marker, so the two are not separate histories. The difference is in how you create them.

| | Save a version | Commit in Changes |
| --- | --- | --- |
| How | `Ctrl+Alt+S` (`Cmd+Option+S`), **File > Save a version** | **Commit** button or `Ctrl+Enter` |
| What is recorded | Every changed file that `.gitignore` does not ignore, whether staged or not | Only the files you staged (or the previous commit, with **Amend**) |
| Name | Optional; defaults to the time, such as *Version of 2026-10-18 14:32* | Required message |
| Shows in the Versions timeline | Yes, with a **Saved version** mark | Yes, without the mark |
| Open files with unsaved edits | Saved first | Commit what is on disk; save first |

Use versions when you want a milestone without thinking about staging. Use commits when you want control over what goes into each change. Because a version stages everything, do not rely on **Staged Changes** to hold some files back from a version. For restoring, see [Version History and Restore](version-history.md): restoring one file or the whole project adds to history and loses nothing.

## How do I share a project with a co-author?

Sharing uses a Git host such as GitHub or GitLab, or your own server, and a remote address you create there.

1. Open **Source control**, then **Versions**.
2. Paste the repository address (`https://…` or `git@…`) into **Share this project with others** and press **Connect**.
3. Save a version, then press **Share**. Manang pushes the current branch and never forces. If someone shared first, it stops and asks you to get updates.
4. To receive their work, press **Get updates**. Unsaved changes are first kept as a version called *Before getting updates*. If only the remote moved, you catch up; if you changed different things, Manang combines them into a new version; if you changed the same lines, it stops without touching your files and lists the files involved.

Manang never asks for or stores a password: sign-in uses your credential helper or the ssh key in your agent. The full screen-by-screen description is in [Share and Get Updates](share-and-get-updates.md).

### What does the Sync button do?

The **Sync** button in the Changes view appears only when the current branch has an upstream, and shows commits ahead and behind. It pulls with fast-forward only. It does not push and it does not merge. To send commits to the remote, use **Share** in the Versions view. To bring in changes that need a merge, use **Get updates**.

## What is not supported?

Verified against the source, these do not exist in Manang:

- Cloning a repository. Run `git clone` in a terminal, then choose **File > Open folder…**.
- A merge-conflict editor. A conflicting **Get updates** stops before changing files.
- Stash, rebase, tags, branch deletion and force push.
- Staging by line or hunk. You stage whole files.
- A blame or file-history view in the interface.
- Submodules. Network commands do not recurse into them.
- Live co-editing through Git. Live sessions are a separate feature; see [Edit Together Live](edit-together-live.md).

Large and binary files are handled like any other, with a plain diff message instead of lines. Very large files are better kept out of the project.

## Which should I use for a thesis or paper?

For a solo thesis, save a named version at each milestone and share to a private repository as an off-machine backup; see [Write a Thesis in Typst](write-a-thesis-in-typst.md). For a team that already uses Git conventions, use the Changes view and branches, and use **Share** and **Get updates** for the exchange. Both coexist on the same repository.

## Sources

- Manang source: `crates/manang-git` and `docs/adr/0005-git-backend.md` in the project repository.

Last checked: October 2026.
