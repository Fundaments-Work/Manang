# Projects and Files

You can organize project directories in the tree view, preview images and PDFs, launch external system applications, delete files safely, use Home trash, and track revisions with Git.

## 1. Use the Files view

The Files sidebar displays your project's folder structure as an accessible tree.

1. Open the Files view:
   - Click the **Files** icon in the Activity bar.
   - Or choose **View > Files** from the menu bar.
   - Or press `Ctrl+Shift+E` (`Cmd+Shift+E`).
   - Toggle the entire sidebar on or off with `Ctrl+B` (`Cmd+B`).
2. Search and filter files:
   - Type in the filter field at the top of the panel to quickly filter visible items.
   - To fuzzy-find and open any file by name across the project, press `Ctrl+P` (`Cmd+P`) or choose **Go > Go to file**.
3. Create new files and folders:
   - **New file:** Click the **+** (New file) icon in the header, choose **File > New file**, or press `Ctrl+N` (`Cmd+N`). Type the file name and press `Enter`.
   - **New folder:** Click the **New folder** icon in the header, type the directory name, and press `Enter`.
4. Rename files:
   - Right-click any file or folder and select **Rename**.
   - Type the new name and press `Enter`, or press `Escape` to cancel.
5. Set main file:
   - In projects with multiple documents, right-click your root document (`main.typ` or `main.tex`) and choose **Set as main file**. Manang compiles this file regardless of which tab you currently view.

## 2. Open any file type

Manang routes files to the appropriate viewer based on their format:

1. **Text documents (`.typ`, `.tex`, `.md`, `.bib`, `.json`, `.toml`, `.yaml`, `.txt`):**
   - Open directly in the CodeMirror text editor.
2. **Vector figures (`.figure`, `.figure.json`):**
   - Open in the interactive figure editor canvas with vector drawing tools and bound connectors.
3. **Images (`.png`, `.jpg`, `.jpeg`, `.gif`, `.webp`, `.svg`):**
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
3. Manang moves the item to your operating system's Recycle Bin or Trash (`files.trash-on-delete: true` by default). Unsaved tab changes in the deleted file are discarded.
4. To restore an accidentally deleted file, open your operating system's Trash or Recycle Bin folder.

### Manage projects with Home trash
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
2. If your project folder is not yet a Git repository:
   - Click **Initialize repository**. Manang sets up a clean Git repository in your project directory.
3. Inspect pending changes:
   - Changes are categorized into **Changes**, **Staged Changes**, and **Untracked**.
   - Click any modified file to open an interactive diff view displaying exact line additions and deletions.
4. Stage and unstage files:
   - Click the **+** (Stage) button next to a file, or click **Stage all** on the section header.
   - Click **-** (Unstage) to move a file back to the unstaged changes list.
5. Discard changes:
   - Click the trash icon next to an unstaged file to discard modifications and revert to the last commit.
   - Confirm the prompt to prevent accidental data loss.
6. Commit changes:
   - Enter a descriptive summary in the commit message text box.
   - (Optional) Check the **Amend** box to update the most recent commit.
   - Click **Commit** (or press `Ctrl+Enter` (`Cmd+Enter`)).
7. Manage branches:
   - Click the branch dropdown button at the top of the panel to see local branches.
   - Select another branch to check it out.
   - Or type a new name and click **Create branch** to branch your work.
8. Synchronize with remote repositories:
   - Click **Sync** in the panel header to pull upstream commits and push local commits to your remote Git forge (e.g. GitHub, GitLab).
9. View commit history:
   - Scroll through previous commits in the history list. Click **Load more** to inspect earlier revisions.
