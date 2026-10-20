# Version History and Restore

Save named versions of your whole project, browse the timeline, compare changes line by line, and restore a file or the entire project safely.

Manang's version history is built for scientific authoring. You capture named milestones of your project, read what changed in each one, and go back to an earlier draft at any time. Versions are stored inside your project with standard Git, so you need no terminal, no third-party software and no cloud account.

## 1. Save a version

Saving a version captures the state of the project's files at that moment.

1. Press `Ctrl+Alt+S` (`Cmd+Option+S`) from anywhere in the application.
   - Or open the command palette (`Ctrl+K` (`Cmd+K`)), type `Save a version`, and press `Enter`.
   - Or choose **File > Save a version**.
   - Or click **Save a version** at the top of the **Versions** view.
2. The first time, Manang may ask two things before it saves:
   - **Start keeping versions?** appears when the project is not tracked yet. Choose **Start keeping versions**. Your files are not changed.
   - **Your name and email** appears when Git does not know who you are. Versions carry the name of whoever saved them. Enter both and choose whether to use them only for this project or for all projects.
3. Manang saves any open files that have unsaved edits. If nothing has changed since the last version, it tells you and saves nothing.
4. In the **Save a version** dialog, type a name in **Name this version (optional)**, for example *Pre-submission draft* or *Reviewer feedback incorporated*. If you leave it blank, Manang names it after the time, for example *Version of 2026-10-18 14:32*.
5. Press `Enter` or click **Save**. A notice confirms the new version.

Files that your project's `.gitignore` ignores are not part of a version.

## 2. Browse the timeline

1. Click **Source control** in the Activity bar.
2. Choose **Versions** at the top of the panel (next to **Changes**). You can also choose **View > Versions**.
3. The timeline lists versions from newest to oldest. Each row shows the name, a short identifier, who saved it, when, and how many files changed. Versions you made with **Save a version** carry a **Saved version** mark; other Git commits in the project are listed too.
4. Use **This file** or **All files** to show only the versions that changed the file you have open, or every version. Press **Load more** at the end of the list to see older versions.

The Versions view also holds the controls for sharing with co-authors; see [Share and Get Updates](share-and-get-updates.md).

## 3. Inspect changes

1. Click a version in the timeline to open it. It lists every file that version added, changed or removed, with the number of lines added and removed.
2. Click a file name to see its changes below the list. Added and removed lines are tinted, and old and new line numbers are shown beside them.
3. A binary file, or a file that is too large to compare here, says so instead of showing lines.

## 4. Restore an earlier version

You can restore a single file or the whole project. Restoring never rewrites history: no version is lost.

### Restore one file
1. Open the version in the timeline and click the file you want.
2. Click **Restore this file**.
3. Read the confirmation and click **Restore**.

### Restore the whole project
1. Open the version you want to return to.
2. Click **Restore whole project**.
3. Read the confirmation. Every file in that version is replaced by its content from that version.
4. If you have unsaved changes to those files, the button reads **Save my changes first, then restore**: Manang first saves your current changes as a version named *Before restoring* followed by the identifier of the version you are restoring. Choose it to continue.
5. Files you created after that version, which it does not contain, are left as they are.
6. Open tabs and the file tree refresh with the restored contents. A notice says how many files were restored.

Before a restore replaces a file on disk, Manang copies the old file into `.git/manang-trash/` inside your project, so there is a second safety copy.

If you are in a live editing session, restoring the live file is refused until you leave the session; see [Edit Together Live](edit-together-live.md).
