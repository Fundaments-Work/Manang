# Projects and Files

Manage files in the project tree, open images and PDFs, move files to the trash safely, and use the Source control panel to stage, commit and pull with Git.

## 1. Use the Files view

The Files sidebar displays your project's folder structure as an accessible tree.

1. Open the Files view:
   - Click the **Files** icon in the Activity bar.
   - Or choose **View > Files** from the menu bar.
   - Or press `Ctrl+Shift+E` (`Cmd+Shift+E`).
   - Toggle the entire sidebar on or off with `Ctrl+B` (`Cmd+B`).
2. Find files:
   - To fuzzy-find and open any file by name across the project, press `Ctrl+P` (`Cmd+P`), press `/` while no text field has focus, or choose **Go > Go to file**.
   - To search inside files, use the Search panel; see [Search and Replace](search-and-replace.md).
   - The header of the Files panel also has **Refresh files** and **Collapse all folders** buttons.
3. Create new files and folders:
   - **New file:** Click the **+** (New file) icon in the header, choose **File > New file**, or press `Ctrl+N` (`Cmd+N`). Type the file name and press `Enter`.
   - **New folder:** Click the **New folder** icon in the header, type the directory name, and press `Enter`.
4. Rename files:
   - Right-click any file or folder and select **Rename**.
   - Type the new name and press `Enter`, or press `Escape` to cancel.
   - Other items in the same menu are **Open**, **New file**, **New folder**, **Move to trash** and **Copy path**.
5. Set main file:
   - In projects with multiple documents, right-click your root document (for example `main.typ` or `main.tex`) and choose **Set as main file**. The choice is saved as `main` in the project's `manang.toml`, and Manang compiles that file regardless of which tab you currently view. The item is offered for files of the project's language that are not already the main file.

## 2. Open any file type

Manang routes files to the appropriate viewer based on their format. [Supported File Types](file-types.md) has the full list.

1. **Text documents (`.typ`, `.tex`, `.md`, `.bib`, `.json`, `.toml`, `.yaml`, `.txt` and many more):**
   - Open directly in the CodeMirror text editor. Text files larger than 5 MiB open in a read-only viewer instead.
2. **Vector figures (`.figure`, `.figure.json`):**
   - Open in the interactive figure editor canvas with vector drawing tools and bound connectors.
3. **Images (`.png`, `.jpg`, `.jpeg`, `.gif`, `.webp`, `.svg`, `.bmp`, `.avif`, `.ico`):**
   - Open in a dedicated File Viewer tab.
   - View dimensions and file size in the top toolbar.
   - Click the image (or click the **1:1** toolbar button) to switch between fit-to-window and full original resolution.
   - For `.svg` vector files, click **Open as text** in the toolbar to inspect or edit the underlying SVG markup.
4. **PDF documents (`.pdf`):**
   - Open in the built-in PDF viewer tab.
   - Select and copy text, search with `Ctrl+F` (`Cmd+F`), follow links, navigate the document outline, use continuous anchored zoom, toggle dark pages, and jump to source. See [Preview and PDF](preview-and-pdf.md) for full details.
5. **Open with system app:**
   - For binary files (such as spreadsheets, CAD files, or archives) or to open an image in an external graphics editor, click the **Open with system app** icon button in the viewer toolbar. Manang launches the file using your operating system's default associated application.

## 3. Delete files and manage Home trash

Manang ensures that both individual files and entire authoring projects can be deleted safely and recovered when needed.

### Delete individual files safely
1. Right-click any file or directory in the Files tree.
2. Select **Move to trash**.
3. Manang first saves any unsaved edits in the file, then moves the item into the project's own trash folder, `.manang/trash/<id>/`, keeping its original path. It does not go to your operating system's Recycle Bin.
4. To restore an accidentally deleted file, copy or move it back from `.manang/trash/` in your project folder. See [The Trash](trash.md).

### Manage projects with Home trash
Deleting projects from the Home screen works differently from deleting files; the steps are summarized here and explained in [The Trash](trash.md).
1. To remove a project from your workspace:
   - On the Home screen, right-click any project card (or click its menu button) and choose **Move to trash**.
   - If the project is currently open in the workbench, Manang prompts you to close the project and save any pending changes before moving it to trash.
2. View trashed projects:
   - Click the **Trash** section at the bottom of the Home screen sidebar.
   - Trashed projects display relative deletion timestamps (for example, *Trashed 5 minutes ago*).
   - Direct opening of trashed projects is disabled to protect against unintended edits.
3. Restore a project:
   - Click the **Restore** button on any trashed project card.
   - The project immediately returns to your active project list.
4. Purge or delete permanently:
   - Click the **Delete** button on a trashed project card.
   - A confirmation dialog presents two distinct options:
     - **Remove from Manang:** Removes the project from Manang's project registry, leaving all files on disk intact.
     - **Move folder to system trash:** Moves the entire project directory on disk to your operating system's trash.
5. Empty trash:
   - Click the **Empty trash** button in the Trash section header to purge all trashed project entries at once.

For detailed workflows on deleting and restoring projects, see [The Trash](trash.md). For capturing milestone snapshots and reverting project states, see [Version History and Restore](version-history.md).

## 4. Use the Source Control view

Manang includes a built-in Git client powered by the pure-Rust `gix` engine.

1. Open the **Source control** panel by clicking the branch icon in the Activity bar (or choose **View > Source control** from the menu bar).
2. The panel has two parts, **Changes** and **Versions**; this section describes **Changes**, and [Version History and Restore](version-history.md) describes **Versions**.
3. If your project folder is not yet a Git repository:
   - Click **Initialize repository**. Manang sets up a clean Git repository in your project directory.
4. Inspect pending changes:
   - Changes are categorized into **Changes**, **Staged Changes**, and **Untracked**.
   - Click any modified file to open an interactive diff view displaying exact line additions and deletions.
5. Stage and unstage files:
   - Click the **+** (Stage) button next to a file, or click **Stage all** on the section header.
   - Click **-** (Unstage) to move a file back to the unstaged changes list.
6. Discard changes:
   - Click the trash icon next to an unstaged file to discard modifications and revert to the last commit.
   - Confirm the prompt to prevent accidental data loss.
7. Commit changes:
   - Enter a descriptive summary in the commit message text box.
   - (Optional) Check the **Amend** box to update the most recent commit.
   - Click **Commit** (or press `Ctrl+Enter` (`Cmd+Enter`)).
8. Manage branches:
   - Click the branch dropdown button at the top of the panel to see local branches.
   - Select another branch to check it out.
   - Or click **+ Create new branch…**, type a name and click **Create** to branch your work.
9. Synchronize with remote repositories:
   - Click **Sync** in the panel header to pull upstream commits from your remote (e.g. GitHub, GitLab). The pull only fast-forwards, so it stops if your branch has diverged, and **Sync** does not push your commits. To send your work out, use [Share and get updates](share-and-get-updates.md) or push from a terminal. See [Use Manang with Git](use-manang-with-git.md) for what the panel does and does not do.
10. View commit history:
   - Scroll through previous commits in the history list. Click **Load more** to inspect earlier revisions.
