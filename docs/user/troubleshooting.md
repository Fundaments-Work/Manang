# Troubleshooting

Find the Manang log files, open developer tools, fix LaTeX and Linux display problems, understand plugin load failures, and report a bug to the team.

## 1. Locate application log files

Manang writes diagnostic messages, compiler events, slow commands, and panics to platform log files using `tauri-plugin-log`. Log files rotate automatically when they reach 2 MB.

Find the log file for your operating system (identifier `work.fundaments.manang`):

- **Linux:**
  `~/.local/share/work.fundaments.manang/logs/Manang.log`  
  *(or `$XDG_DATA_HOME/work.fundaments.manang/logs/Manang.log` if custom XDG variables are set)*
- **macOS:**
  `~/Library/Logs/work.fundaments.manang/Manang.log`
- **Windows:**
  `%LOCALAPPDATA%\work.fundaments.manang\logs\Manang.log`  
  *(typically `C:\Users\<Username>\AppData\Local\work.fundaments.manang\logs\Manang.log`)*

You can also choose **Help > Open logs** to open the log folder in your file manager.

To follow logs live in a terminal while running on Linux or macOS:
```bash
tail -f ~/.local/share/work.fundaments.manang/logs/Manang.log
```

## 2. Open developer tools in development builds

When running a development build of Manang (such as with `pnpm app:dev` or `cargo tauri dev`), you can open the built-in web developer tools to inspect UI state, network requests, and console errors.

1. In the Manang window, press `F12`.
   - Or press `Ctrl+Shift+I` (`Cmd+Shift+I`).
2. The web inspector opens.
3. Switch to the **Console** tab to view frontend log messages and exceptions.
4. Switch to the **Elements** or **Network** tabs to inspect layout and asset loading.
5. *Note:* In production release builds, developer tools are disabled for security and performance.

## 3. "No TeX installation found" error

### Problem
When you open or compile a LaTeX document, the preview pane remains empty and the **Problems** panel (`Ctrl+Shift+M` (`Cmd+Shift+M`)) displays:
`No TeX installation found: install TeX Live or MiKTeX so that latexmk or pdflatex is on your PATH.`

### Cause
Manang compiles LaTeX using your operating system's TeX distribution (`latexmk`, `pdflatex`, `xelatex`, or `lualatex`). If these executables are not found in your system `PATH`, LaTeX compilation cannot start.

### Solution
1. Install TeX Live or MiKTeX:
   - **Ubuntu / Debian:** `sudo apt install texlive-latex-extra latexmk`
   - **Fedora:** `sudo dnf install texlive-scheme-medium latexmk`
   - **Arch Linux:** `sudo pacman -S texlive-meta`
   - **macOS:** `brew install --cask mactex-no-gui` (or download MacTeX from [tug.org/mactex](https://tug.org/mactex/))
   - **Windows:** Install TeX Live from [tug.org/texlive](https://tug.org/texlive/) or MiKTeX from [miktex.org](https://miktex.org/). Ensure the installer option to add binaries to `PATH` is enabled.
2. Restart Manang after installation so the desktop app inherits the updated `PATH` environment variable.
3. In Manang Settings (`Ctrl+,` (`Cmd+,`)), verify under **Plugins > LaTeX** that **LaTeX engine** is set to **Automatic** or **System TeX**.
4. *Alternative, no TeX needed:* press **Download the built-in engine** in the preview pane, the Problems tab or Settings > **LaTeX**. See "No TeX installed? Use the built-in engine" in [Preview and PDF](preview-and-pdf.md).

### The built-in engine will not download or run
- **"Could not reach the download server":** check your internet connection and any firewall or proxy, then press **Try again**. The download continues where it stopped.
- **"The downloaded file did not match its checksum":** nothing was installed. Try again later; if it repeats, tell us (Help > Send feedback).
- **"The downloaded engine cannot start on this computer":** the engine needs a few system libraries that your Linux does not have. Install TeX Live instead, or tell us which distribution you use.
- **The option is not shown:** this version of Manang has no built-in engine for your platform yet. Install TeX Live or MiKTeX.
- **A package is "not found" with the built-in engine, or "The built-in engine's TeX files are not downloaded yet":** the engine fetches packages on first use. Check your connection, and that **Fetch LaTeX packages on demand** is on in Settings > **LaTeX**; then compile again.
- **Reset everything:** Settings > **LaTeX** > **Remove the built-in engine**, then download it again.

## 4. Blank or stuttering window on Linux Wayland with NVIDIA

### Problem
When launching Manang on Linux under a Wayland session using proprietary NVIDIA graphics drivers, the window may flicker, stutter, or display a blank surface.

### Cause
This is caused by a known hardware-acceleration incompatibility between WebKitGTK's DMABUF renderer and proprietary NVIDIA Wayland drivers.

### Solution
1. **Automatic workaround:** Manang automatically detects the combination of Wayland and NVIDIA drivers at startup and disables WebKit's DMABUF renderer (`WEBKIT_DISABLE_DMABUF_RENDERER=1`). This ensures smooth, glitch-free rendering without manual configuration.
2. **Manual override:** If you want to force DMABUF rendering on (for example, if you are testing a newer driver release that resolves the issue), launch Manang with the variable set to `0`:
   ```bash
   WEBKIT_DISABLE_DMABUF_RENDERER=0 pnpm app:dev
   ```

## 5. A plugin fails to compile or load

### Problem
A third-party plugin shows **Failed** in the Plugins list or an error message. See "What the status words mean" in [Plugins](plugins.md).

### Solution
1. **Check the directory name:** The directory in `manang/plugins/<name>/` inside Manang's configuration folder (see [Plugins](plugins.md)) must exactly match the `name` field in `manifest.toml`.
2. **Validate manifest syntax:** Ensure `manifest.toml` is valid TOML and uses valid permissions:
   - Buffer: `read:active`, `write:active`
   - Filesystem: `read:<dir>`, `write:<dir>`
   - Network: exact hostnames (e.g. `api.crossref.org`) without `https://` prefixes or wildcards.
3. **Check imports and compilation target:** The plugin WebAssembly component must be compiled for `wasm32-unknown-unknown` against the Manang component interfaces. If a component imports host functions that are not granted in `[permissions]`, Manang rejects the component with an `UngrantedImport` error.
4. **Inspect log details:** Open `Manang.log` (see paths above) to view the exact error trace reported by the WebAssembly runtime.

## 6. Report an issue

If you discover a bug or unexpected behavior:

1. Choose **Help > Report a problem** (or **Help > Send feedback** for an idea). A dialog opens.
2. Describe what happened in a few lines.
3. Leave **Include diagnostics** on to attach a report with Manang's version, your plugins and the end of the log. Home folders and anything that looks like a key are removed, your documents are never included, and you can read the whole report in the dialog.
4. Click **Copy**, then send the text to the Manang team, for example in an issue on GitHub at `https://github.com/Fundaments-Work/Manang/issues`. Manang sends nothing by itself.
5. Also note your operating system version, desktop environment (Wayland or X11 on Linux), graphics hardware and the steps to reproduce the issue. **Open logs folder** in the dialog opens the full log if you want to attach it.
