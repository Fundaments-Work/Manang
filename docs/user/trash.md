# The Trash

Move projects and files to the Trash instead of deleting them, restore them with one click, and know where Manang keeps its internal backup copies.

## 1. Project trash on the Home screen

The Home screen includes a dedicated Trash view that tracks removed projects without deleting files from your hard drive until you confirm.

### Move a project to trash
1. In the Home screen project grid, locate the project card.
2. Click the card's menu button (or right-click the card) and select **Move to trash**.
3. If the project is currently open in the workbench, Manang prompts you to close the project and save any pending changes first.
4. The project disappears from your active projects list and moves to the Trash view.

### View trashed projects
1. Click **Trash** at the bottom of the Home screen sidebar.
2. The Trash view displays all trashed projects alongside their relative deletion timestamps (for example, *Trashed 10 minutes ago*).
3. To protect against unintended edits, opening a project directly from the Trash view is disabled.

### Restore a trashed project
1. In the Trash view, locate the project you want to recover.
2. Click the **Restore** button on the project card.
3. The project immediately returns to your active projects list with all files, figures, and settings intact.

### Permanently purge or delete a project
1. In the Trash view, click the **Delete** button on a project card.
2. A confirmation dialog presents two distinct removal choices:
   - **Remove from Manang:** Removes the project from Manang's project registry and recent history, leaving all files on your physical drive untouched. Use this if you want to clean up your Home screen but keep the folder.
   - **Move folder to system trash:** Moves the entire project folder on disk to your operating system's Recycle Bin or Trash.
3. To purge all trashed projects simultaneously, click the **Empty trash** button in the top-right header of the Trash view.

## 2. Delete and recover individual files

You can delete individual files and folders within an open project safely from the Files sidebar.

### Delete a file or folder
1. In the workbench sidebar, click the **Files** icon to display the file tree.
2. Right-click the file or folder you want to remove and select **Move to trash**.
3. If the file has unsaved edits in an open tab, Manang saves the latest edits first, so the trashed file contains your latest work.
4. Manang moves the item into the project's own trash folder, `.manang/trash/<id>/`, under its original path. A notice confirms the move. The item does not go to your operating system's Recycle Bin.

### Restore a deleted file
1. In your file manager, open the `.manang` folder inside your project (it is a hidden folder, so turn on hidden files if you do not see it).
2. Open `trash`, then the numbered folder for that deletion, and find the file or folder under its original path.
3. Move or copy it back to its original directory inside your project.
4. Press **Refresh files** in the Files panel if the tree does not update on its own.

## 3. Project internal backup folders

In addition to the operating system trash, Manang maintains internal safety backup folders within your project directory:

- **`.manang/trash/`:** Deleted files, as described above, and the originals of files that Manang is about to overwrite or replace: a project-wide replace (see [Search and Replace](search-and-replace.md)), **Convert to Typst** over an existing file, **Keep my version** after a change on disk, and files replaced or deleted during a live session. Each operation gets its own numbered subfolder.
- **`.git/manang-trash/`:** When you discard uncommitted changes in the Source Control panel or restore an earlier version snapshot, replaced files are backed up here first.

These folders are local, never synced to remote repositories, and provide an offline safety net against unintended overwrites.
