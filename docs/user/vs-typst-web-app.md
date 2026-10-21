# Manang vs the Typst Web App

Manang is an offline desktop editor that compiles Typst locally, while the Typst web app is the browser-based editor made by the Typst team.

## Short answer

Both use a Typst compiler, so your `.typ` files are the same text in either place. The Typst web app runs in a browser, is built for sharing and teamwork, and is the Typst team's own product. Manang is a desktop app that keeps your project as files on your disk and also edits LaTeX and Markdown. Choose the web app for browser access and shared projects with commenting, and Manang for offline work, files you control and a combined workspace. They do not conflict: you can move a Typst project between them as a folder of files.

## Manang and the Typst web app side by side

Typst web app facts link to Typst's own pages. Manang facts come from its code and the other pages of this guide. "Not stated" means the sources read do not say.

| | Manang | Typst web app |
| :--- | :--- | :--- |
| Where it runs | Desktop app for Windows, macOS and Linux ([Install Manang](install.md)). Public beta | In the browser; it lets you "work collaboratively on Typst projects in your browser" ([Typst docs](https://typst.app/docs/)). A self-hosted edition, Typst On-Premises, exists ([open source page](https://typst.app/open-source/)) |
| Works offline | Yes: the compiler is built in and nothing needs the network to write, preview or export ([Typst editor](typst-editor.md)) | Not stated in the web app documentation read; Typst's pages point to the open source command-line compiler for local use ([open source page](https://typst.app/open-source/)) |
| Languages | Typst, LaTeX, Markdown | Typst. The pricing page lists conversion from LaTeX, Word, Markdown and more ([pricing](https://typst.app/pricing/)) |
| Compile engine | The Typst compiler (version 0.15.1 in this release), run on your computer | Typst's compiler; the open source compiler is Apache-2.0 ([open source page](https://typst.app/open-source/)). Which version a project uses: not stated on the pages read |
| Live preview | Preview beside the editor, rebuilt after a short pause; source and preview jump to each other | Editor and preview panels by default ([Export and Preview](https://typst.app/docs/web-app/export-and-preview/)) |
| Export | PDF; Markdown can be converted to Typst first ([Preview and PDF](preview-and-pdf.md)) | PDF, PNG, SVG and HTML (HTML is described as experimental) ([Export and Preview](https://typst.app/docs/web-app/export-and-preview/)) |
| Collaboration | Live co-editing of one text file, end-to-end encrypted, no accounts (whole-project sharing arrives in the next release); no comments ([Edit Together Live](edit-together-live.md)) | Share by link or email with edit or view access, and teams ([Typst home](https://typst.app/home/), [Concepts](https://typst.app/docs/web-app/concepts/)). Comments attach to text and depend on the project owner's plan ([Comments](https://typst.app/docs/web-app/comments/)) |
| Version history | Named versions stored with Git; restore a file or the project ([Version History](version-history.md)) | Git sync with GitHub or GitLab, which needs a paid plan ([Git Sync](https://typst.app/docs/web-app/git-sync/)). A built-in history browser: not stated in the documentation read |
| Reference management | References panel for `.bib` and CSL-JSON; optional online Verify ([References](references.md)) | Sync a bibliography with Zotero or Mendeley; creating one needs a paid plan ([Reference Sync](https://typst.app/docs/web-app/reference-sync/)) |
| Packages | Never downloads packages; reads packages already in Typst's local data and cache folders ([how packages work](typst-editor.md#how-do-typst-packages-work-in-manang)) | Community packages and templates from Typst Universe; private packages need a paid plan ([pricing](https://typst.app/pricing/), [Private Packages](https://typst.app/docs/web-app/private-packages/)) |
| AI | Your own key; proposals you accept or reject ([AI Assistant](ai-assistant.md)) | Not stated. The pricing page lists "Checks" described as assists for compiler errors, without naming AI |
| Extensibility | Sandboxed WebAssembly plugins with declared permissions ([Plugins](plugins.md)) | Typst packages and templates; editor extensions: not stated for the web app |
| Licence and openness | MIT | The compiler is Apache-2.0; the web app is proprietary and not open source ([open source page](https://typst.app/open-source/)) |
| Where your files live | A folder on your disk | Typst's servers. The pricing page's security section says web app data is stored in a German data centre ([pricing](https://typst.app/pricing/)) |

For plans, prices and limits, see Typst's [pricing page](https://typst.app/pricing/). They change, so this page states none.

## Will my Typst project behave the same in both?

Mostly, but check three things. First, compiler version: Manang bundles one version (0.15.1 in this release), and the web app's version for a given project is not stated on the pages read. Second, packages: the web app can use Typst Universe packages, while Manang never downloads packages and only finds ones already in Typst's data or cache folders, so an `@preview` import that works in the web app may fail in Manang until the package is on your computer ([details](typst-editor.md#how-do-typst-packages-work-in-manang)). Third, fonts: Typst's own docs warn that a project archive downloaded from the web app "may fail to compile or look different when compiled locally" and that it leaves out web-app-provided fonts ([Export and Preview](https://typst.app/docs/web-app/export-and-preview/)); Manang uses the fonts bundled with it and does not scan your system fonts in this release. Compile once in the new place and compare the PDFs.

## Can I move a project from the web app to Manang?

Yes, as files. Download the project archive from the web app, unzip it into a folder and add a small `manang.toml` containing `engine = "typst"` and your main file, then use **File > Open folder**. The exact file is shown in [Coming from Overleaf](coming-from-overleaf.md#bring-a-typst-project-over); the steps are the same for any source. Private packages are not included in the archive ([Export and Preview](https://typst.app/docs/web-app/export-and-preview/)).

## Choose Manang if

- You want to write and compile with no connection and no account.
- You want your project as plain files you back up and version yourself.
- You also write LaTeX or Markdown and want one app for all of them.
- You want live co-editing that is end-to-end encrypted and needs no accounts, accepting that Manang has no comments.
- You want an AI assistant with your own key that never applies a change without you.

## Choose the Typst web app if

- You want to start in a browser with nothing to install, on any computer.
- Co-authors or reviewers should join with a link, work in the browser and leave comments.
- You rely on Typst Universe packages and want them available without managing folders.
- You want the Typst team's own hosted product, including features such as Zotero or Mendeley sync, presentation mode or on-premises hosting if your organisation needs them.
- You prefer a hosted product over a public beta.

## Using both

You can draft or review in the web app and work offline in Manang on the same `.typ` files, with a Git repository as the bridge if you like. Typst's web app can link a project to a GitHub or GitLab repository on a paid plan ([Git Sync](https://typst.app/docs/web-app/git-sync/)), and Manang can share and get updates from a repository address ([Share and Get Updates](share-and-get-updates.md)). Manang does not connect to the web app directly.

## Sources

Claims about the Typst web app and compiler come from Typst's own pages. Claims about Manang come from its code and the other pages in this guide.

- [https://typst.app/docs/](https://typst.app/docs/): the web app, the separately installed command-line compiler, compiler version of the docs.
- [https://typst.app/open-source/](https://typst.app/open-source/): compiler under Apache-2.0, web app proprietary, On-Premises, packages and sandboxing in the CLI.
- [https://typst.app/home/](https://typst.app/home/): sharing by link or email, teams, Git version control.
- [https://typst.app/pricing/](https://typst.app/pricing/): which features sit in which plan (feature names only), LaTeX conversion, German data centre.
- [https://typst.app/docs/web-app/concepts/](https://typst.app/docs/web-app/concepts/): projects, sharing link, team roles, private packages, fonts.
- [https://typst.app/docs/web-app/export-and-preview/](https://typst.app/docs/web-app/export-and-preview/): preview, export formats, project archive caveats.
- [https://typst.app/docs/web-app/comments/](https://typst.app/docs/web-app/comments/): comments and the plan they depend on.
- [https://typst.app/docs/web-app/git-sync/](https://typst.app/docs/web-app/git-sync/): GitHub and GitLab sync and its plan requirement.
- [https://typst.app/docs/web-app/private-packages/](https://typst.app/docs/web-app/private-packages/): private packages and the paid plan.
- [https://typst.app/docs/web-app/reference-sync/](https://typst.app/docs/web-app/reference-sync/): Zotero and Mendeley sync.
- [https://github.com/typst/typst](https://github.com/typst/typst): Typst compiler repository, Apache-2.0, web app description.

Last checked: October 2026.
