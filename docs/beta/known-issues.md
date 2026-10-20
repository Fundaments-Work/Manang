# Known Issues & Workarounds

This is a living document of known issues, platform quirks, and workarounds verified in the current beta build of Manang.

---

## Summary Table

| Issue | Subsystem | Platforms | Severity | Workaround Available |
| :--- | :--- | :--- | :--- | :--- |
| [1. Local TeX distribution required for LaTeX](#1-local-tex-distribution-required-for-latex) | LaTeX Compiler | All | Medium | Yes |
| [2. Math formulas in Mermaid text diagrams](#2-math-formulas-in-mermaid-text-diagrams) | Diagrams | All | Low | Yes |
| [3. Linux Wayland + NVIDIA display stutter](#3-linux-wayland--nvidia-display-stutter) | Desktop Shell | Linux | Low | Yes (Automatic) |
| [4. macOS native top menu bar integration](#4-macos-native-top-menu-bar-integration) | Menus | macOS | Low | Yes |
| [5. Plugin signatures for local installs](#5-plugin-signatures-for-local-installs) | Plugins | All | Low | Yes |
| [6. Real-time co-editing](#6-real-time-co-editing) | Collaboration | All | Low | Yes |
| [7. AI assistant needs your own key](#7-ai-assistant-needs-your-own-key) | AI Assistant | All | Low | Yes |
| [8. Files changed on disk by another program](#8-files-changed-on-disk-by-another-program) | Project / Buffer | All | Low | Yes |

---

## 1. Local TeX Distribution Required for LaTeX

### What
When compiling a LaTeX document (`.tex`), Manang delegates to the operating system's TeX distribution (`latexmk`, `pdflatex`, `xelatex`, or `lualatex`). If no TeX installation is on your `PATH`, compilation fails with:  
`No TeX installation found: install TeX Live or MiKTeX so that latexmk or pdflatex is on your PATH.`

### How to Avoid
1. Install a standard TeX distribution:
   - **Debian / Ubuntu:** `sudo apt install texlive-latex-extra latexmk`
   - **Fedora:** `sudo dnf install texlive-scheme-medium latexmk`
   - **Arch Linux:** `sudo pacman -S texlive-meta`
   - **macOS:** `brew install --cask mactex-no-gui`
   - **Windows:** Install TeX Live or MiKTeX and ensure executables are on `PATH`.
2. Or let Manang download its optional built-in engine (about 48 MB) when it offers it, in the preview pane, the Problems tab or Settings > **LaTeX**. It is XeTeX only, fetches LaTeX packages from the internet on first use, and is not available for every platform yet (see below).
3. Restart Manang after installing TeX to ensure updated environment variables are inherited.

### Status
The optional built-in engine (Tectonic, XeTeX only) is available as a consented download on **Linux x86-64 and arm64** in this beta; Windows and macOS builds are planned. It runs as a separate process, checks its download against a checksum shipped inside Manang, and needs the network once per new package. `pdflatex`-only packages, LuaLaTeX and `biber` need a real TeX installation. A self-contained WebAssembly engine is still being investigated.

---

## 2. Math Formulas in Mermaid Text Diagrams

### What
Mermaid flowcharts (`.mmd` files or Markdown diagram fences) support alphanumeric text and standard symbols in node labels, but complex LaTeX or Typst mathematical formulas (such as integrals or fractions) are not rendered within Mermaid nodes.

### How to Avoid
For diagrams requiring publication-grade mathematical notation, use the **Vector Figure Studio** (`.figure` / `/draw`), which features dedicated Math nodes (`M` key) evaluated directly through the Typst and LaTeX formula engines.

### Status
Math formula editor is fully operational in Figure Studio; inline LaTeX/Typst formula support in Mermaid labels is planned for a future update.

---

## 3. Linux Wayland + NVIDIA Display Stutter

### What
Under Wayland sessions using proprietary NVIDIA drivers, WebKitGTK's hardware-accelerated DMABUF renderer can experience surface stuttering or flickering.

### How to Avoid
Manang automatically detects Linux Wayland + NVIDIA at launch and disables the DMABUF renderer (`WEBKIT_DISABLE_DMABUF_RENDERER=1`), ensuring smooth 60 FPS rendering. If you wish to test with hardware DMABUF acceleration enabled, launch with:
```bash
WEBKIT_DISABLE_DMABUF_RENDERER=0 pnpm app:dev
```

### Status
Automatic mitigation is active and verified in all desktop builds.

---

## 4. macOS Native Top Menu Bar Integration

### What
On macOS, Manang currently renders its custom header menu bar (File, Edit, View, Go, Build, Help) within the window header rather than hoisting menu items into the native macOS system menu bar at the top of the display.

### How to Avoid
Use window menus directly or standard macOS keyboard shortcuts (`Cmd+K`, `Cmd+,`, `Cmd+E`, `Cmd+Shift+V`).

### Status
Native macOS menu bar bridge is planned after the first beta.

---

## 5. Plugin Signatures for Local Installs

### What
Plugins from the **Browse** tab come from the signed registry at `registry.fundaments.work`: the index is signed, every archive is checked against its size and SHA-256, and the permissions in the archive must match the signed index. Plugins you install from a local folder or a `.manangplugin` file are not signed; only the permission prompt stands between you and them (they still run in the WebAssembly sandbox).

### How to Avoid
Install local plugins only from sources you trust, and read the permission prompt (folders, network hosts) before confirming.

### Status
By design. The registry is new and starts empty; first-party plugins arrive in the next releases.

---

## 6. Real-time Co-Editing

### What
Live co-editing (see [Edit together live](../user/edit-together-live.md)) shares **one text file** or the **whole project** per session. It uses the hosted relay at `wss://relay.fundaments.work` by default (end-to-end encrypted; the relay sees only ciphertext and connection metadata). **Get updates** and restoring the live file are refused during a session; leave first. Joining a single file or project with copies that differ from the host's replaces your copies with the host's, only after asking, saving the old copies (single file: `live-trash` in Manang's settings folder; project: `.manang/trash` in the project) and, for a project, after one confirmation when more than one file is replaced.

Whole-project sessions have limits:

- **Anyone with the invite can create, change and delete shared files in your project folder** (never outside it, never dotfiles, `.git`, `manang.toml`, `.manang`, `target`, `node_modules`, `__pycache__` or ignored files). There are no accounts and no read-only role, and nobody can be removed from a session: leave and start a new one.
- **Images, figures and other non-text files are last-writer-wins**; concurrent edits of one figure lose one side.
- **A session ends when the last person leaves.** There is no room to rejoin; a joiner to an empty room keeps waiting for a host that is not there.
- A large rewrite on disk (a Git checkout) is held and you are asked whether to share it or put your folder back.
- Windows and macOS case-folding of file names is implemented and tested on Linux only.

### How to Avoid
Send a whole-project invite only to people you would let edit the project, and keep Git commits of your own before a session. For anything else, use Git through the **Versions** view (View menu) or a shared folder.

### Status
Beta: one file or the whole project. Merging a differing copy instead of replacing it, member roles and removal, comments, a room that outlives its members, and merging figure edits are planned. Design and threat model: ADR 0014.

---

## 7. AI Assistant Needs Your Own Key

### What
The assistant proposes edits as diffs and never changes a file without your approval. You pick a provider (Anthropic, OpenAI, DeepSeek, OpenRouter, Groq, or a local Ollama) and, for hosted ones, give an API key that is stored in your system keychain. Nothing is sent until you choose a provider, consent and press Send.

### How to Avoid
Nothing to avoid; set a provider in the assistant panel or Settings when you want it, or switch the agent off. Compile-error fixes that do not need AI are offered separately (the **Fix** actions in Problems).

### Status
Available in this beta. Provider coverage and prompts are still being tuned.

---

## 8. Files Changed on Disk by Another Program

### What
If a file open in Manang is changed or removed by another program (Git on the command line, a sync client), Manang tells you and keeps your unsaved text; it does not merge automatically. Autosave and crash recovery protect your edits meanwhile.

### How to Avoid
Save before running external Git operations that rewrite project files, and choose Keep mine or Reload when Manang asks.

### Status
Handled in this beta; a three-way merge helper is planned.
