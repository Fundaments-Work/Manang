# Manang vs TeXstudio

Manang is a desktop editor for Typst, LaTeX and Markdown, while TeXstudio is an open source desktop editor built specifically for writing LaTeX.

## Short answer

Both are desktop programs that edit files on your own disk, and both hand LaTeX to a TeX installation on your computer. TeXstudio is a LaTeX specialist with a large set of editing helpers, macros and scripting, and it has been developed since 2009. Manang covers Typst and Markdown as well as LaTeX, has a built-in Typst compiler, a figure studio, an AI assistant that only proposes changes, and live co-editing, but it is a public beta. If you write only LaTeX and want a mature, scriptable editor, TeXstudio is a sound choice; if you want Typst or a combined workspace, try Manang.

## Manang and TeXstudio side by side

TeXstudio facts link to its own pages. Manang facts come from its code and the other pages of this guide. "Not stated" means the sources read do not say.

| | Manang | TeXstudio |
| :--- | :--- | :--- |
| Where it runs | Desktop: Windows, macOS, Linux ([Install Manang](install.md)). Public beta | Desktop: "Windows, Unix/Linux, BSD, and Mac OS X" ([texstudio.org](https://texstudio.org/)) |
| Works offline | Yes for writing, preview and export; optional features such as live editing, Verify and Git updates use the network | Local program. The AI assistant needs a cloud provider or a local model, and the grammar checker needs a separately installed LanguageTool ([configuration](https://texstudio-org.github.io/configuration.html), [advanced](https://texstudio-org.github.io/advanced.html)) |
| Languages | Typst, LaTeX, Markdown | LaTeX; Typst and Markdown are not mentioned in its manual |
| Compile engine | Typst compiler built in. LaTeX through your TeX Live, MiKTeX or MacTeX, or an optional Tectonic (XeTeX) engine on Linux only ([offline LaTeX](offline-latex-editor.md)) | Calls your installed LaTeX system, `pdflatex` by default, with latexmk and custom commands; a LaTeX system "is not provided by TeXstudio" ([getting started](https://texstudio-org.github.io/getting_started.html)) |
| Live preview | PDF preview rebuilds after a short pause (300 ms by default); Typst, LaTeX and Markdown alike | Integrated PDF viewer with "(almost) word-level syncing" and live inline preview of formulas ([texstudio.org](https://texstudio.org/)). Rebuilding on every pause: not stated |
| Source and PDF jumps | Both ways (SyncTeX) | Ctrl+click in the PDF jumps to source, and F7 scrolls the PDF to the cursor position ([getting started](https://texstudio-org.github.io/getting_started.html)); external viewers can be configured ([configuration](https://texstudio-org.github.io/configuration.html)) |
| Collaboration | Live co-editing of one text file, end-to-end encrypted (whole-project sharing arrives in the next release); no track changes or comments ([Edit Together Live](edit-together-live.md)) | Collaborative editing "with the help of the program teamtype", which you download separately and which the page lists for macOS and Linux ([configuration](https://texstudio-org.github.io/configuration.html)) |
| Version history | Named versions stored with Git, restore file or project ([Version History](version-history.md)) | "some simple actions for subversion/git", including an optional automatic check-in after every save and undo back through earlier revisions ([configuration](https://texstudio-org.github.io/configuration.html)). A separate history browser: not stated |
| Reference management | References panel for `.bib` and CSL-JSON; optional online Verify ([References](references.md)) | Bibliography menu and citation completion; the manual suggests JabRef for comfortable reference management ([advanced](https://texstudio-org.github.io/advanced.html)) |
| AI | Your own key, proposals you accept or reject, nothing applied automatically ([AI Assistant](ai-assistant.md)) | AI chat assistant: Mistral AI, ChatGPT (marked "untested" on the page), Claude, OpenRouter or local models; "both questions and selected text is sent to that provider" (local models do not leak information to providers). An Insert button puts the answer into the editor, and with tool use, on by default, the model can call functions such as `replace_selected_text` ([advanced](https://texstudio-org.github.io/advanced.html)). Whether each edit asks for confirmation: not stated |
| Extensibility | Sandboxed WebAssembly plugins with declared permissions ([Plugins](plugins.md)) | Macros, scripting with QJS (ECMAScript-based), user commands and `.cwl` completion files ([advanced](https://texstudio-org.github.io/advanced.html)) |
| Licence | MIT | Open source. The website says GPL v2 ([texstudio.org](https://texstudio.org/)); the GitHub repository shows GPL v3 ([GitHub](https://github.com/texstudio-org/texstudio)) |
| Where your files live | A folder on your disk | Files on your disk (a desktop program); the AI conversation it stores is on disk too, and storing it can be disabled ([advanced](https://texstudio-org.github.io/advanced.html)) |

TeXstudio's [releases page](https://github.com/texstudio-org/texstudio/releases) showed 4.9.8 as the latest release when checked. Manang is at v0.1.0-beta.1.

## Does Manang need a TeX installation like TeXstudio does?

For LaTeX, yes, with one exception. On Windows and macOS you install TeX Live, MiKTeX or MacTeX yourself, as with TeXstudio. On Linux (x86-64 and arm64) Manang can also download an optional built-in engine, Tectonic, which is XeTeX only: it has no `biber`, no LuaLaTeX and does not run packages that only work with `pdflatex`. Typst needs nothing installed. See [How does LaTeX compile in Manang?](offline-latex-editor.md#how-does-latex-compile-in-manang).

## Can Manang use my existing LaTeX project?

Yes. A LaTeX project is a folder of ordinary files, so open the folder in Manang after adding a small `manang.toml` project file. [Coming from Overleaf](coming-from-overleaf.md#bring-a-latex-project-over) shows the file; the steps are the same for a folder from any source. This guide documents no import of TeXstudio settings or macros.

## Choose Manang if

- You write Typst or Markdown as well as LaTeX and want them in one window.
- You want figures drawn and diagrams rendered inside the editor ([Figures and Diagrams](figures-and-diagrams.md)).
- You want an AI assistant that shows exactly what it will send and applies nothing without your approval.
- You want live co-editing with end-to-end encryption, without accounts.
- You are comfortable with a public beta and its unsigned installers.

## Choose TeXstudio if

- You write only LaTeX and want a mature editor with a long feature list: structure view, spell and grammar checking, math symbol panels, assistants for tables and images.
- You want macros and scripting to automate your editing.
- You want a build setup you define yourself: TeXstudio documents custom build commands and `latexmk`, and Manang runs LaTeX with shell escape off.
- You prefer an established project over a beta.

## Using both

They work on the same plain files, so you can open one folder in either. Keep your TeX distribution shared between them. Keep in mind that Manang adds a `manang.toml` file to a project, and may create a `.manang` folder inside it.

## Sources

Claims about TeXstudio come from its own site, manual and repository. Claims about Manang come from its code and the other pages in this guide.

- [https://texstudio.org/](https://texstudio.org/): platforms, feature list, PDF viewer sync, licence stated as GPL v2, origin in 2009.
- [https://github.com/texstudio-org/texstudio](https://github.com/texstudio-org/texstudio): README feature list; repository licence GPL v3.
- [https://github.com/texstudio-org/texstudio/releases](https://github.com/texstudio-org/texstudio/releases): latest release shown when checked.
- [https://texstudio-org.github.io/getting_started.html](https://texstudio-org.github.io/getting_started.html): a separate LaTeX system is required; `pdflatex` by default; compile and view keys.
- [https://texstudio-org.github.io/advanced.html](https://texstudio-org.github.io/advanced.html): macros, QJS scripting, `.cwl` files, bibliography menu, AI chat assistant and its privacy note.
- [https://texstudio-org.github.io/configuration.html](https://texstudio-org.github.io/configuration.html): latexmk, external viewers, spelling dictionaries, grammar checking with LanguageTool, git and SVN actions, collaboration through teamtype.

Last checked: October 2026.
