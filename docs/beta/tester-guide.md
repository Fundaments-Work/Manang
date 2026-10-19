# Manang Beta Tester Guide

Welcome to the private, invite-only beta of **Manang**—the high-performance scientific authoring and illustration suite combining Typst, LaTeX, Markdown, an in-house vector figure studio, and citation indexing.

This guide explains what to expect during the beta, how to get started in your first 15 minutes, how to report friction or bugs, and our uncompromising privacy promise.

---

## 1. What the Beta Is (and What to Expect)

The Manang beta is an early release for researchers, authors, and students across **Linux, macOS, and Windows**. During this phase, we are evaluating real-world performance, stability, and authoring workflows.

Here is what you should expect:
1. **Interfaces will evolve:** Based on your feedback, toolbars, shortcuts, and panel layouts will be refined.
2. **Data safety & backups:** While Manang performs atomic saves and places deleted files in recoverable trash, **you should keep your own backups (such as in Git or your cloud folder) until we declare general availability.**
3. **Respect for your files:** Manang never overwrites, alters, or moves your files without your explicit action.
4. **Features in flight:** Some capabilities described in our long-term roadmap (such as cloud remote sharing) are actively being built and are marked as **coming in a later beta** throughout the interface.

---

## 2. Installation and Updates

> **Note on Installers:** Native installers for Linux (`.deb`, `.rpm`, tarball), macOS (`.dmg`) and Windows (`.msi`, `.exe`) are on the GitHub release. They are not code-signed yet, so macOS Gatekeeper and Windows SmartScreen warn on the first launch; the install guide explains the two clicks that get past it.

### Future In-App Updates
Manang includes an automated beta update channel that checks for weekly bugfix releases using revocable per-tester private tokens. When an update is ready, Manang displays a release notification and summary ("What's New") before updating.

---

## 3. The First 15 Minutes: An Interactive Walk-Through

To explore Manang's core capabilities, walk through this quick five-step tour:

### Step 1: Explore the Home View
Launch Manang. You will see the **Home** view displaying your project workspace:
- **Projects & Recent:** Jump into recent workspaces.
- **Templates:** Pre-configured starters for Typst articles, slides, posters, letters, LaTeX documents, and Markdown notes.
- **Trash:** Recoverable project storage. Deleting a project never destroys your data unexpectedly.

### Step 2: Create a Project from the "Welcome" Starter
1. Click **New project** (or select **Templates** from the sidebar).
2. Choose **Welcome to Manang** (`templates/welcome/`) and pick a directory on your machine.
3. Click **Create project**. The workspace opens immediately into `main.typ`.

### Step 3: Experience Sub-Millisecond Live Preview
1. In `main.typ`, edit any sentence or change an equation under `= Mathematical Typesetting`.
2. Notice the live preview panel on the right updates instantly without visible lag or flicker.

### Step 4: Test Bidirectional SyncTeX Navigation
1. In the preview panel, double-click or `Ctrl`-click (`Cmd`-click on macOS) any heading or sentence.
2. The text editor caret jumps directly to the matching source line.
3. Press `Ctrl+Alt+J` (`Cmd+Alt+J` on macOS) to jump forward from your editor cursor back to the preview.

### Step 5: Test the Command Palette & Settings
1. Press `Ctrl+K` (`Cmd+K` on macOS) to open the Command Palette. Type `export` to see export commands, or type `/` to browse tools.
2. Press `Ctrl+,` (`Cmd+,` on macOS) to open **Settings**.
3. Toggle between **User** and **Project** scopes, or try switching between Dark and Light themes.
4. Press `Ctrl+/` (`Cmd+/` on macOS) to open the **Keyboard Shortcuts** reference sheet.

5. **No TeX on your computer? (LaTeX testers)** Open a LaTeX project. The preview says "No TeX installation found" and offers **Download the built-in engine (about 14 MB)**. Please try it: read the window, press **Download**, watch the progress (try **Cancel** once too), and check that the document then compiles. Tell us how long the first compile took and whether any package was missing. Remove it again in Settings > **LaTeX** if you like.
6. Try **Save a version** (File menu, or `Ctrl+Alt+S`; `Cmd+Option+S` on macOS): it saves a named snapshot of your project. Open **Versions** (View menu) to see the timeline, compare a version with your files, and restore a file or the whole project. Restore never deletes anything, and it saves your current changes first if you ask it to.

6. Try **Share this file live** (File menu). It needs a relay address first (Settings, section Live). Send the invite to a second tester, who chooses **Join a live session**. See [Edit together live](../user/edit-together-live.md).

*(Sharing through a git remote is coming in a later beta.)*

---

## 4. How to Report a Bug, Confusion, or Ideas

Because Manang has no telemetry, we rely completely on what you choose to report.

### 4.1. Check Known Issues First
Before filing a ticket, please check [known-issues.md](known-issues.md) to see if your issue or platform quirk is already documented with a workaround.

### 4.2. File a Ticket on GitHub
Visit our issue tracker at:  
[https://github.com/Fundaments-Work/Manang/issues](https://github.com/Fundaments-Work/Manang/issues)

Choose the relevant form:
- **Bug Report:** For unexpected errors, compile failures, rendering glitches, or crashes.
- **Beta Feedback ("Felt Slow or Confusing"):** For subjective observations—e.g. if typing felt laggy, a shortcut was unintuitive, or an interaction was confusing.
- **Feature Request:** For missing scientific authoring workflows or formatting tools.

### 4.3. What to Attach
To help us diagnose issues quickly, please provide:
1. Your operating system (e.g. Ubuntu 24.04 Wayland, macOS Sequoia Apple Silicon, Windows 11).
2. Your Manang version or commit hash (found in **Help > About**).
3. The type of document being authored (Typst, LaTeX, or Markdown).
4. Relevant error lines from your local `Manang.log` file (see below).

---

## 5. What We Collect (Zero Telemetry)

**Manang collects zero automated telemetry.**  
There are no analytics trackers, no crash pings, and no background network calls scanning your files.

### Manual Log File Locations
If an error occurs, diagnostic logs are written exclusively to your local storage:

- **Linux:**  
  `~/.local/share/work.fundaments.manang/logs/Manang.log`  
  *(or `$XDG_DATA_HOME/work.fundaments.manang/logs/Manang.log`)*
- **macOS:**  
  `~/Library/Logs/work.fundaments.manang/Manang.log`
- **Windows:**  
  `%LOCALAPPDATA%\work.fundaments.manang\logs\Manang.log`  
  *(typically `C:\Users\<Username>\AppData\Local\work.fundaments.manang\logs\Manang.log`)*

Logs automatically rotate when they reach 2 MB. An in-app diagnostics bundle generator ("Report a problem") that packages logs and sanitizes settings for you to inspect before sending is **coming in a later beta**.

---

## 6. The Privacy Promise in Plain Words

1. **Your documents stay on your machine:** Your manuscripts, drafts, notes, figures, and bibliographies are stored as standard UTF-8 files in folders you choose. Nothing is uploaded to remote servers.
2. **Local compilation:** Typst and Markdown compilation runs 100% locally in-process. LaTeX compiles using your local TeX distribution or an isolated local worker. If you have no TeX installed, Manang can download its optional built-in LaTeX engine, but only after you press Download in a window that names the server, size and checksum. That engine then fetches LaTeX packages from `relay.fullyjustified.net` the first time a document uses them (you can turn that off in Settings > LaTeX). Manang never downloads the engine without your consent.
3. **AI privacy when it lands:** When AI features arrive, they are strictly bring-your-own-key (BYOK). Manang will feature an explicit context tray displaying every single token that will be sent before any network request occurs, and keys are stored securely in your operating system keychain.
