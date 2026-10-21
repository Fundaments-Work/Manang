# Manang vs VS Code with LaTeX Workshop

Manang is a standalone desktop editor for Typst, LaTeX and Markdown, while LaTeX Workshop adds LaTeX building and preview to the VS Code editor.

## Short answer

These are different kinds of tool. LaTeX Workshop is an extension: it turns VS Code, a general-purpose code editor, into a LaTeX environment, so you keep everything else VS Code offers, including other extensions. Manang is a single application built for documents in Typst, LaTeX and Markdown, with a preview, figure studio, references panel and AI assistant included. If you already live in VS Code and write LaTeX, LaTeX Workshop is the natural fit. If you want a document-focused app with Typst built in and fewer parts to assemble, try Manang, remembering that it is a public beta.

## Manang and VS Code with LaTeX Workshop side by side

LaTeX Workshop and VS Code facts link to their own pages. Manang facts come from its code and the other pages of this guide. "Not stated" means the sources read do not say.

| | Manang | VS Code with LaTeX Workshop |
| :--- | :--- | :--- |
| Where it runs | Desktop app for Windows, macOS and Linux ([Install Manang](install.md)). Public beta | VS Code on the desktop, plus Remote Development setups ([Remote](https://github.com/James-Yu/LaTeX-Workshop/wiki/Remote)). The FAQ lists vscode.dev, github.dev and code-server as not supported ([FAQ](https://github.com/James-Yu/LaTeX-Workshop/wiki/FAQ)) |
| Works offline | Yes for writing, preview and export ([offline LaTeX](offline-latex-editor.md)) | Builds with a TeX distribution on your PATH ([Install](https://github.com/James-Yu/LaTeX-Workshop/wiki/Install)). Offline behaviour of VS Code itself: not covered here |
| Languages | Typst, LaTeX, Markdown | LaTeX, with Julia and R literate documents ([Extra features](https://github.com/James-Yu/LaTeX-Workshop/wiki/ExtraFeatures)). Typst and Markdown are not part of this extension; Typst has a separate extension, [Tinymist](https://github.com/Myriad-Dreamin/tinymist) |
| Compile engine | Typst compiler built in. LaTeX through your TeX distribution, or an optional Tectonic (XeTeX) engine on Linux only | Your TeX distribution: "the only requirement is a compatible LaTeX distribution in the system PATH"; the default recipe needs `latexmk` ([Install](https://github.com/James-Yu/LaTeX-Workshop/wiki/Install)). Recipes and tools include `latexmk`, `xelatexmk`, `lualatexmk`, `pdflatex`, `bibtex` and `tectonic` ([Compile](https://github.com/James-Yu/LaTeX-Workshop/wiki/Compile)) |
| Live preview | PDF preview rebuilds after a short pause | Auto build can run never, on save, or when a dependency changes (the default, `onFileChange`), with PDF preview in VS Code or a browser ([Compile](https://github.com/James-Yu/LaTeX-Workshop/wiki/Compile), [README](https://github.com/James-Yu/LaTeX-Workshop)) |
| Source and PDF jumps | Both ways (SyncTeX) | Both ways (SyncTeX) ([README](https://github.com/James-Yu/LaTeX-Workshop)) |
| Collaboration | Live co-editing of one text file, end-to-end encrypted (whole-project sharing arrives in the next release); no track changes or comments ([Edit Together Live](edit-together-live.md)) | Visual Studio Live Share is supported but "uses workarounds that may cause issues" ([FAQ](https://github.com/James-Yu/LaTeX-Workshop/wiki/FAQ)); the PDF viewer and compiler must run on the host ([Extra features](https://github.com/James-Yu/LaTeX-Workshop/wiki/ExtraFeatures)) |
| Version history | Named versions stored with Git, restore file or project ([Version History](version-history.md)) | Not stated by LaTeX Workshop |
| Reference management | References panel for `.bib` and CSL-JSON; optional online Verify ([References](references.md)) | Completion of bibliography keys and labels ([README](https://github.com/James-Yu/LaTeX-Workshop)). A reference manager: not stated |
| AI | Your own key; proposals you accept or reject; one switch turns it off ([AI Assistant](ai-assistant.md)) | None in LaTeX Workshop as far as its pages say. VS Code has GitHub Copilot, which can use a Copilot plan, your own API key or a local model ([VS Code docs](https://code.visualstudio.com/docs/copilot/overview)) |
| Extensibility | Sandboxed WebAssembly plugins with declared permissions ([Plugins](plugins.md)) | VS Code extensions; authors choose their own licences ([VS Code FAQ](https://code.visualstudio.com/docs/supporting/faq)) |
| Licence | MIT | LaTeX Workshop is MIT ([Marketplace](https://marketplace.visualstudio.com/items?itemName=James-Yu.latex-workshop)). VS Code's source is MIT as Code - OSS, while the VS Code product ships under a Microsoft product licence ([VS Code FAQ](https://code.visualstudio.com/docs/supporting/faq)) |
| Data location | A folder on your disk; no account | Files on your disk. VS Code sends usage and crash data to Microsoft unless you turn telemetry off; extensions are not controlled by that setting ([VS Code FAQ](https://code.visualstudio.com/docs/supporting/faq)) |

This page states no prices or plan limits. Check the vendors' own pages for current terms, including any AI plan.

## Do I need a TeX distribution with either?

For LaTeX, in both cases yes. LaTeX Workshop requires one on your PATH, and TeX Live is its recommended choice ([Install](https://github.com/James-Yu/LaTeX-Workshop/wiki/Install)). Manang uses TeX Live, MiKTeX or MacTeX when it finds one, and on Linux (x86-64 and arm64) it can also download an optional Tectonic engine, which is XeTeX only and has no `biber`. Typst needs no TeX in Manang. See [How does LaTeX compile in Manang?](offline-latex-editor.md#how-does-latex-compile-in-manang).

## Can I write Typst in VS Code?

Yes, with a separate extension. Tinymist is described as an integrated language service for Typst with a VS Code extension, export on save and a preview server ([Tinymist](https://github.com/Myriad-Dreamin/tinymist)). It is not part of LaTeX Workshop, so a VS Code setup for both languages means installing and configuring two extensions. Manang edits Typst with its compiler built in ([Using Manang as a Local Typst Editor](typst-editor.md)).

## Choose Manang if

- You write Typst or Markdown as well as LaTeX and want one app for all three.
- You want figures, references and a PDF viewer in the app without choosing and configuring extensions.
- You want live co-editing with end-to-end encryption and no accounts, without Live Share workarounds.
- You want an AI assistant that shows what it will send and applies nothing without your approval.

## Choose VS Code with LaTeX Workshop if

- You already use VS Code and want LaTeX inside the same editor as your code, notes and other projects.
- You want VS Code extensions and settings: other languages, linters, remote development over SSH or in containers, and extensions you already use.
- You want fine control over build recipes, magic comments or an external build command ([Compile](https://github.com/James-Yu/LaTeX-Workshop/wiki/Compile)).
- You need Julia or R literate documents inside LaTeX ([Extra features](https://github.com/James-Yu/LaTeX-Workshop/wiki/ExtraFeatures)).
- You prefer an established extension over a beta; the Marketplace page lists a large install base.

## Using both

Both work on plain files in a folder, so you can open the same project in each. Manang adds a `manang.toml` file to a project, which VS Code and LaTeX Workshop ignore; see [Supported File Types](file-types.md#7-the-manangtoml-project-file).

## Sources

Claims about LaTeX Workshop and VS Code come from their own pages. Claims about Manang come from its code and the other pages in this guide.

- [https://github.com/James-Yu/LaTeX-Workshop](https://github.com/James-Yu/LaTeX-Workshop): README, features: build on save, PDF preview, SyncTeX, IntelliSense, linting.
- [https://github.com/James-Yu/LaTeX-Workshop/wiki/Install](https://github.com/James-Yu/LaTeX-Workshop/wiki/Install): TeX distribution on PATH, `latexmk` for the default recipe.
- [https://github.com/James-Yu/LaTeX-Workshop/wiki/Compile](https://github.com/James-Yu/LaTeX-Workshop/wiki/Compile): recipes, tools, auto build settings, magic comments, external build command.
- [https://github.com/James-Yu/LaTeX-Workshop/wiki/Remote](https://github.com/James-Yu/LaTeX-Workshop/wiki/Remote): Remote Development support and warnings.
- [https://github.com/James-Yu/LaTeX-Workshop/wiki/FAQ](https://github.com/James-Yu/LaTeX-Workshop/wiki/FAQ): unsupported web editors, Live Share status.
- [https://github.com/James-Yu/LaTeX-Workshop/wiki/ExtraFeatures](https://github.com/James-Yu/LaTeX-Workshop/wiki/ExtraFeatures): Live Share limitations, word count, literate documents.
- [https://marketplace.visualstudio.com/items?itemName=James-Yu.latex-workshop](https://marketplace.visualstudio.com/items?itemName=James-Yu.latex-workshop): extension description and MIT licence.
- [https://code.visualstudio.com/docs/supporting/faq](https://code.visualstudio.com/docs/supporting/faq): VS Code licensing, telemetry and Marketplace terms.
- [https://code.visualstudio.com/docs/copilot/overview](https://code.visualstudio.com/docs/copilot/overview): GitHub Copilot in VS Code and its model options.
- [https://github.com/Myriad-Dreamin/tinymist](https://github.com/Myriad-Dreamin/tinymist): Typst language service and VS Code extension.

Last checked: October 2026.
