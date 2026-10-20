# Search and Replace

Search every project file by text or name, narrow results with filters and regular expressions, and replace across files with a backup of each changed file.

## 1. Search the project

1. Open the **Search** panel in any of these ways:
   - Click **Search** in the Activity bar.
   - Press `Ctrl+Shift+F` (`Cmd+Shift+F`).
   - Choose **Edit > Search in project** or **View > Search**.
2. Type in the box. The box searches for text inside your files and, at the same time, for matching file names. Results update a moment after you stop typing.
3. Read the results:
   - **File names** lists up to 20 files whose names match. Click one to open it.
   - Below it, matches are grouped by file, with the matching lines shown. Click a line to open the file at that line.
   - A summary line says how many matches were found in how many files, and notes when the results were truncated.
4. Use the **Collapse all** (or **Expand all**) button in the panel header to fold the groups, or the arrow beside a file name to fold one.

## 2. Narrow the search

Three toggles sit at the right of the search box:

| Toggle | Shortcut | What it does |
| :--- | :--- | :--- |
| **Match case** | `Alt+C` | Treat upper and lower case as different |
| **Match whole word** | `Alt+W` | Match only complete words |
| **Use regular expression** | `Alt+R` | Read the query as a regular expression |

If a regular expression is not valid, the panel shows **Search syntax error** with the reason and a **Turn off regex** button.

To limit which files are searched, open **Files to include / exclude** and enter comma-separated patterns, for example `*.typ, *.md` under **Files to include** or `*.svg, dist/*` under **Files to exclude**. If nothing is found, **Reset search filters** clears every toggle and pattern.

Search always skips the `.git` and `.manang` folders and anything your project's `.gitignore` ignores. Two settings under **Settings > Plugins > Search** change the rest:

- **Exclude globs** (`search.exclude`): patterns that are left out of every search and replace, such as `build/` or `*.log`.
- **Include hidden files** (`search.include-hidden`, off by default): also search files and folders whose names start with a dot.

## 3. Replace across files

1. In the Search panel, click the arrow at the left of the search box to show the **Replace** box.
2. Enter the text to search for, then the replacement. With **Use regular expression** on, the replacement can refer to captured groups.
3. Check the results so that only the matches you want remain, using the toggles and the file filters. Save any open files with unsaved changes first.
4. Press **Replace all**.

**Replace all** takes effect at once, in every file that has a match, and there is no confirmation step. Before it changes a file, Manang copies the original into a timestamped folder under `.manang/trash/` inside your project, and the notice that follows names that folder. If a file cannot be changed, the notice names it. After a replacement, the search runs again so the panel shows what is left.

To undo a replacement, copy the files back from the backup folder, or restore them from a saved version. See [Version History and Restore](version-history.md) and [Crash Recovery and Safety Copies](crash-recovery.md).

## 4. Find within one document

To search only the file you are editing, press `Ctrl+F` (`Cmd+F`). Choose **Edit > Replace** to open find and replace in the active editor. In a PDF, `Ctrl+F` opens the PDF find bar; see [Preview and PDF](preview-and-pdf.md).
