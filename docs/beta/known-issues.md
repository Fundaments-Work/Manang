# Known Issues & Workarounds

This is a living document of known issues, platform quirks, and workarounds verified in the current beta build of Manang.

---

## Summary Table

| Issue | Subsystem | Platforms | Severity | Workaround Available |
| :--- | :--- | :--- | :--- | :--- |
| [1. Local TeX distribution required for LaTeX](#1-local-tex-distribution-required-for-latex) | LaTeX Compiler | All | Medium | Yes |
| [2. Math formulas in Mermaid text diagrams](#2-math-formulas-in-mermaid-text-diagrams) | Diagrams | All | Low | Yes |
| [3. Linux Wayland + NVIDIA display stutter](#3-linux-wayland-nvidia-display-stutter) | Desktop Shell | Linux | Low | Yes (Automatic) |
| [4. macOS native top menu bar integration](#4-macos-native-top-menu-bar-integration) | Menus | macOS | Low | Yes |
| [5. Plugin cryptographic signature verification](#5-plugin-cryptographic-signature-verification) | Plugins | All | Medium | Yes |
| [6. Real-time co-editing & remote sharing](#6-real-time-co-editing-remote-sharing) | Collaboration | All | Low | Coming soon |
| [7. AI agent assistant view](#7-ai-agent-assistant-view) | AI Assistant | All | Low | Coming soon |
| [8. Unsaved external disk change prompt](#8-unsaved-external-disk-change-prompt) | Project / Buffer | All | Medium | Coming soon |

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
2. Alternatively, compile the optional embedded Tectonic worker:
   ```bash
   cargo build -p manang-latex --features engine --bin manang-latex-worker
   ```
3. Restart Manang after installing TeX to ensure updated environment variables are inherited.

### Status
Investigating self-contained WebAssembly and standalone sidecar TeX distribution bundles for Wave 1.

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
Native macOS menu bar bridge is scheduled for platform polish before public Beta 1 release.

---

## 5. Plugin Cryptographic Signature Verification

### What
The plugin manager (`Ctrl+Shift+X`) permits installing third-party plugins from local folders or `.manangplugin` zip archives. While capability consent prompts (checking filesystem and network host permissions) are enforced, cryptographic signature verification (`plugin.sig`) is not yet mandatory.

### How to Avoid
Only install plugins from trusted sources and verified authors. Always review the permission prompt (e.g. read/write directories, network domains) before confirming installation.

### Status
Ed25519 plugin signature verification is scheduled for Wave 5.

---

## 6. Real-time Co-Editing & Remote Sharing

### What
Live co-editing (see [Edit together live](../user/edit-together-live.md)) works on **one text file per session** and needs a relay address. There is no hosted relay yet: someone has to run `manang-relay`, or you point Manang at one you were given. Figures, PDFs and several files at once are not shared. Cloud sync does not exist.

While you are in a session, do not use **Get updates** on the live file (Manang does not block it yet). Joining with a copy that differs from the host's replaces your copy with the host's, after asking and after saving yours outside the project. Anyone holding the invite can edit and can use any name.

### How to Avoid
Use standard asynchronous Git collaboration via the **Source Control** side panel (`Ctrl+Shift+G`) for everything else, or store projects in shared network directories.

### Status
Beta 1: text files only. Merging a differing copy with the host's instead of replacing it, figures, and a hosted relay are planned.

---

## 7. AI Agent Assistant View

### What
The AI writing assistant, context tray, and automated compiler-error diff repairs are not yet present in the primary user interface.

### How to Avoid
Write and format documents using standard editing features. Check compiler diagnostics in the **Problems** panel (`Ctrl+Shift+M`).

### Status
Foundation crates (`manang-agent-core`, `manang-keystore`, `manang-agent-rig`) are complete; UI view integration is scheduled for Wave 2.

---

## 8. Unsaved External Disk Change Prompt

### What
If a file currently open in Manang is modified simultaneously by an external program (e.g. Git command line, external script, or file synchronizer), the buffer does not yet pop up a merge/conflict prompt.

### How to Avoid
Save your files (`Ctrl+S`) before running external Git operations or running scripts that modify project files in place.

### Status
Data safety snapshot engine (`manang-recovery`) is actively being wired into the workspace state.
