# Crash Recovery and Safety Copies

Manang saves recovery snapshots of unsaved edits and keeps safety copies before risky operations, so a crash or a mistake does not cost you your work.

## 1. How crash recovery ("hot exit") works

While any document has unsaved edits, Manang continuously maintains an atomic recovery snapshot in the background.

1. **Atomic background writing:**
   - Edits are debounced and snapshotted automatically (within 2 seconds of your last edit, at most every 10 seconds).
   - Snapshots are written using atomic filesystem operations (temporary file, flush to storage, atomic rename). A sudden crash or power loss during writing never leaves a torn or partial snapshot.
2. **Snapshot storage location:**
   - Snapshots are stored in Manang's application data folder under `recovery/<project-id>/<file-id>.json`:
     - **Linux:** `~/.local/share/work.fundaments.manang/recovery/`
     - **macOS:** `~/Library/Application Support/work.fundaments.manang/recovery/`
     - **Windows:** `%APPDATA%\work.fundaments.manang\recovery\`
3. **Safety limits and isolation:**
   - Snapshots never overwrite or alter your source files on disk.
   - Individual documents larger than 5 MiB are excluded from background snapshotting to avoid disk I/O stalls.
   - If a snapshot cannot be parsed, Manang quarantines it as `<name>.json.corrupt` and continues loading other snapshots.

## 2. Recover your work after an unexpected quit

When you launch Manang and open a project that contains active recovery snapshots, the recovery dialog appears automatically before the workspace opens.

The recovery dialog presents three options:

1. **Recover all:**
   - Loads all snapshotted files into editor tabs as unsaved edits.
   - You can review the recovered changes in your editor tabs and press `Ctrl+S` (`Cmd+S`) to write them to disk, or press `Ctrl+Shift+S` (`Cmd+Shift+S`) to save all files.
2. **Review:**
   - Opens an inspection view showing each recovered file side-by-side with its on-disk counterpart.
   - You can accept or discard recovered edits file by file.
3. **Discard:**
   - Deletes the recovery snapshots and opens the project using the files currently on disk.

Saving a recovered file or explicitly choosing to discard removes its snapshot file from disk.

## 3. Safety copies for destructive operations

Manang creates safety copies before executing destructive actions, ensuring that changes can be rolled back even if you have not configured Git.

### Batch find and replace
When you run a project-wide replace operation across multiple files:
1. Manang writes backup copies of every file about to be modified into a numbered folder under `.manang/trash/` within your project folder.
2. If an error occurs during replacement, the operation halts immediately.
3. You can inspect or recover original versions from `.manang/trash/` at any time.

### Files changed by another program
If a file that is open in Manang changes on disk, for example because another editor saved it or a Git checkout rewrote it:

1. If you have no unsaved edits in that file, Manang reloads it silently.
2. If you do have unsaved edits, a bar appears above the editor and nothing is overwritten. Choose **Keep my version** (your text stays, and the disk version is first copied into `.manang/trash/`), **Load disk version** (press `Ctrl+Z` to get your own text back), or **Compare** to see the differences.
3. If the file was deleted outside Manang, the tab and its text stay open. Choose **Save again** to write the file back, **Save as** to save it under another name, or **Close**.

### Source control discard and checkout
When you discard uncommitted modifications in the Source Control view:
1. Files about to be overwritten are copied into `.git/manang-trash/` before `git checkout` or `git restore` runs.
2. If you accidentally discard modifications, you can recover the files directly from `.git/manang-trash/`.

### Version restore safety commits
When restoring an earlier project milestone (see [Version History and Restore](version-history.md)), Manang checks for uncommitted edits in your working directory. You are prompted to create an automatic safety commit (`safety_version`) before restoring earlier revisions, ensuring that your current draft remains permanently accessible in the version timeline.

### Atomic file saves
Whenever you save a file (`Ctrl+S` / `Cmd+S`), Manang writes the new content to a temporary file in the same directory, flushes data to physical storage, and atomically renames it over the target file. A crash, kernel panic, or power disconnect during a save operation will never leave an empty or truncated file on your drive.
