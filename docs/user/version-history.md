# Version History and Restore

Manang includes a built-in version history system designed specifically for scientific authoring. You can capture named milestone snapshots of your entire project, inspect visual line-by-line diffs between revisions, and restore earlier drafts at any time.

Version history is stored locally within your project using standard Git under the hood, requiring no terminal commands, third-party software, or remote cloud accounts.

## 1. Save a version (Milestone snapshot)

Creating a version captures the exact state of all files and figures across your project at that moment.

1. Press `Ctrl+Alt+S` (`Cmd+Option+S`) from anywhere in the application.
   - Or open the command palette (`Ctrl+K` (`Cmd+K`)), type `Save a version`, and press `Enter`.
   - Or click the **Save a version** icon button in the Source Control panel.
2. Enter an optional version label or description (for example, *Pre-submission draft*, *Reviewer feedback incorporated*, or *Reorganized methods section*).
   - If you leave the label blank, Manang automatically timestamps the version (for example, *Version of 2026-10-18 14:32 UTC*).
3. Press `Enter` or click **Save**.
4. Manang stages all project files and creates a local snapshot commit tagged with `Manang-Version: 1`. A notification confirms the new version with its short commit hash and changed file count.

## 2. Browse the version timeline

1. Click the **Source control** icon in the Activity bar (or press `Ctrl+Shift+G` (`Cmd+Shift+G`)).
2. Select the **Versions** section in the sidebar.
3. The timeline lists every captured version in reverse chronological order:
   - Version label and timestamp.
   - Short commit identifier.
   - Number of files changed.

## 3. Inspect changes and diffs

1. Click any version card in the timeline to expand its changed files list.
2. The panel lists every file added, modified, or removed in that version.
3. Click a file name to open the interactive diff viewer:
   - **Side-by-side or Unified diff:** Toggle between split view and unified view in the diff header.
   - **Syntax highlighting:** Additions are highlighted in green, deletions in red, preserving language syntax highlighting for Typst, LaTeX, and Markdown.
   - **Deterministic figure diffs:** For vector figures (`.figure`), inspect structural changes in shapes, connectors, and properties.

## 4. Restore an earlier version

You can restore your entire project to an earlier snapshot, or restore specific files individually:

### Restore the entire project
1. In the Versions timeline, select the version you wish to restore.
2. Click the **Restore version** button.
3. If you have uncommitted edits in your current workspace, Manang prompts you with a safety confirmation:
   - **Create safety version and restore:** Automatically captures your current work in a new snapshot commit (`safety_version`) before replacing files.
   - **Restore directly:** Overwrites modified files while copying the replaced files to `.git/manang-trash/`.
4. Any new files created since that version that did not exist in the snapshot are preserved untouched.
5. The workspace refreshes, reopening active files with the restored contents.

### Restore individual files
1. Expand the file list for a historical version.
2. Right-click the file you want to restore and choose **Restore file**.
3. Manang creates a safety copy in `.git/manang-trash/` and replaces that single file in your working directory.
