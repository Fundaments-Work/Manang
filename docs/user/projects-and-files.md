# Projects and Files

You can organize project directories in the tree view, preview images and PDFs, launch external system applications, delete files safely, and track revisions with Git.

## 1. Use the Files view

The Files sidebar displays your project's folder structure as an accessible tree.

1. Open the Files view:
   - Click the **Files** icon in the Activity bar.
   - Or press `Ctrl+Shift+E` (`Cmd+Shift+E`).
   - Toggle the entire sidebar on or off with `Ctrl+B` (`Cmd+B`).
2. Search and filter files:
   - Type in the filter field at the top of the panel to quickly filter visible items.
   - To fuzzy-find and open any file by name across the project, press `Ctrl+P` (`Cmd+P`).
3. Create new files and folders:
   - **New file:** Click the **+** (New file) icon in the header, or press `Ctrl+N` (`Cmd+N`). Type the file name and press `Enter`.
   - **New folder:** Click the **New folder** icon in the header, type the directory name, and press `Enter`.
4. Rename files:
   - Right-click any file or folder and select **Rename**.
   - Type the new name and press `Enter`, or press `Escape` to cancel.
5. Set main file:
   - In projects with multiple documents, right-click your root document (`main.typ` or `main.tex`) and choose **Set as main file**. Manang will compile this file regardless of which tab you currently view.

## 2. Open any file type

Manang routes files to the appropriate viewer based on their format.

1. **Text documents (`.typ`, `.tex`, `.md`, `.bib`, `.json`, `.toml`, `.yaml`, `.txt`):**
   - Open directly in the CodeMirror text editor.
2. **Vector figures (`.figure.json`):**
   - Open in the interactive figure editor tab.
3. **Images (`.png`, `.jpg`, `.jpeg`, `.gif`, `.webp`, `.svg`):**
   - Open in a dedicated File Viewer tab.
   - View dimensions and file size in the top toolbar.
   - Click the image (or click the **1:1** toolbar button) to switch between fit-to-window and full original resolution.
   - For `.svg` vector files, click **Open as text** in the toolbar to inspect or edit the underlying SVG markup.
4. **PDF documents (`.pdf`):**
   - Open in the built-in PDF viewer tab with page numbering.
   - *Advanced PDF inspection and annotation tools are coming soon.*
5. **Open with system app:**
   - For proprietary binary files (such as spreadsheets, CAD files, or archives) or to open an image in an external graphics editor, click the **Open with system app** icon button in the viewer toolbar. Manang launches the file using your operating system's default associated application.

## 3. Delete files safely

1. Right-click any file or directory in the Files tree.
2. Select **Move to trash**.
3. Manang moves the item to your operating system's Recycle Bin or Trash (`files.trash-on-delete: true` by default). Unsaved tab changes in the deleted file are discarded.
4. If you accidentally delete a file, open your operating system's Trash or Recycle Bin folder to restore it.
5. *Home trash for restoring deleted projects from within Manang is coming soon.*

## 4. Use the Source Control view

Manang includes a built-in Git client powered by the pure-Rust `gix` engine.

1. Open the **Source control** panel by clicking the branch icon in the Activity bar.
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
