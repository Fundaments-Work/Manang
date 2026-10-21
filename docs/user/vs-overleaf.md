# Manang vs Overleaf

Manang edits Typst, LaTeX and Markdown offline on your own disk, while Overleaf is an online LaTeX editor built around real-time team writing.

## Short answer

Overleaf runs in a web browser and keeps your project on its servers, which suits teams that edit together and review each other's changes. Manang is a desktop app that works on a normal folder of files on your computer, with no account, and adds Typst and Markdown beside LaTeX. Manang is in public beta and has no track changes or review comments. If your supervisor or co-authors need those, Overleaf is the practical choice today; if you mostly write alone or with a few people and want your files and your compile on your own machine, Manang is worth a try on a copy of your project.

## Manang and Overleaf side by side

Overleaf facts link to Overleaf's own pages. Manang facts come from its code and the other pages of this guide. Where a source does not say, the table says "not stated".

| | Manang | Overleaf |
| :--- | :--- | :--- |
| Where it runs | Desktop app for Windows, macOS and Linux ([Install Manang](install.md)). Public beta; installers are not code-signed yet | In the browser. The hosted editor needs a connection to compile ([working offline](https://docs.overleaf.com/writing-and-editing/working-offline)). A self-hosted edition exists ([GitHub](https://github.com/overleaf/overleaf)) |
| Works offline | Yes for writing, preview and export. LaTeX needs a TeX distribution on your computer, or on Linux an optional built-in engine ([offline LaTeX](offline-latex-editor.md)) | Short outages: edits sync on reconnect. Compiling and switching files are not available offline ([working offline](https://docs.overleaf.com/writing-and-editing/working-offline)) |
| Languages | Typst, LaTeX, Markdown | LaTeX; Typst is not mentioned in the [compiler docs](https://docs.overleaf.com/getting-started/recompiling-your-project/selecting-a-tex-live-version-and-latex-compiler) |
| Compile engine | Typst built in. LaTeX through your TeX Live, MiKTeX or MacTeX (`pdflatex`, `xelatex`, `lualatex`), or the optional Tectonic (XeTeX) engine on Linux | pdfLaTeX, LaTeX, XeLaTeX or LuaLaTeX, with a selectable TeX Live version ([docs](https://docs.overleaf.com/getting-started/recompiling-your-project/selecting-a-tex-live-version-and-latex-compiler)). Where the compile runs: not stated on the recompile page |
| Live preview | PDF preview that rebuilds after a short pause, with source-to-PDF jumps both ways | Recompile button and PDF viewer ([docs](https://docs.overleaf.com/getting-started/recompiling-your-project)) |
| Real-time collaboration | Live editing of one text file over an end-to-end encrypted relay (whole-project sharing arrives in the next release); no accounts, no roles ([Edit Together Live](edit-together-live.md)) | Real-time editing, project sharing and chat ([docs](https://docs.overleaf.com/collaborating/collaborating-in-overleaf)) |
| Track changes and comments | None | Both are listed; track changes is a premium feature ([docs](https://docs.overleaf.com/collaborating/track-changes)) |
| Version history | Named versions stored with Git in the project; restore a file or the project ([Version History](version-history.md)) | History with labels, compare and restore. Full project history is a premium feature ([docs](https://docs.overleaf.com/writing-and-editing/history-and-versioning)) |
| Git | Standard Git inside your folder; share and get updates with a repository address ([Share and Get Updates](share-and-get-updates.md)) | Git integration and GitHub sync are premium features on Overleaf Cloud ([docs](https://docs.overleaf.com/integrations-and-add-ons/git-integration-and-github-synchronization)) |
| References | Panel reads `.bib` and CSL-JSON, inserts citations, optional online Verify ([References](references.md)) | Syncs with ReadCube, Zotero and Mendeley; a premium feature ([docs](https://docs.overleaf.com/integrations-and-add-ons/reference-manager-integrations)) |
| AI | Your own key and provider; proposals you accept or reject; one switch turns it off ([AI Assistant](ai-assistant.md)) | AI assistant, error help and writing tools; allowance depends on plan; can be turned off in account settings ([docs](https://docs.overleaf.com/integrations-and-add-ons/ai-features)) |
| Extensibility | Sandboxed WebAssembly plugins with declared permissions ([Plugins](plugins.md)) | Not stated on the pages read; see [Integrations and add-ons](https://docs.overleaf.com/) |
| Licence and openness | MIT | Overleaf's code repository is AGPL v3 ([GitHub](https://github.com/overleaf/overleaf)); the hosted service's own terms are not covered by that page |
| Where your files live | A folder on your disk | Overleaf servers for the hosted service; edits are sent to the server every few seconds ([docs](https://docs.overleaf.com/collaborating/collaborating-in-overleaf)). Server location: not stated on the pages read |

For prices and plan limits, see Overleaf's [plans page](https://www.overleaf.com/user/subscription/plans) and [documentation](https://docs.overleaf.com/). They change, so this page states none.

## Can Manang open my Overleaf project?

Yes, if you download the source files from Overleaf. Manang opens a folder, not an Overleaf link, and it needs a small `manang.toml` file that an Overleaf download does not contain. The steps, and what to expect from package versions and bibliography tools, are in [Coming from Overleaf](coming-from-overleaf.md#can-i-open-my-overleaf-project-in-manang).

## Does Manang have real-time collaboration like Overleaf?

It has live co-editing, but it is not the same thing. In v0.1.0-beta.1 you can share one text file (whole-project sharing arrives in the next release); co-authors see each other's carets and edits, and the relay in the middle cannot read the text. There are no accounts or roles, nobody can be set to read-only, a session ends when the last person leaves, and everyone needs Manang installed. There are no review comments and no track changes. The details are in [How does collaboration differ from Overleaf?](coming-from-overleaf.md#how-does-collaboration-differ-from-overleaf) and [Edit Together Live](edit-together-live.md).

## Choose Manang if

- You want your project as plain files in a folder you control, backed up or versioned your own way.
- You need to write without a connection, or you do not want an account to write.
- You want Typst or Markdown next to LaTeX in one app, or you are moving to Typst.
- You write alone or with a few co-authors who can install software and work together live or through Git.
- You want an AI assistant that uses your own key, shows what it will send, and never applies a change without you.

## Choose Overleaf if

- A larger team edits the same project and needs accounts, read-only access, review comments or track changes.
- Co-authors cannot or will not install software and need only a browser.
- You rely on an Overleaf-hosted template, a publisher integration or another Overleaf workflow.
- You do not want to install or maintain TeX on your own computer. On Windows and macOS, Manang needs a TeX distribution that you install for LaTeX.
- Your project needs `biber`, LuaLaTeX or `pdflatex`-only packages and you do not want to set up a full TeX distribution; Manang's built-in engine does not cover them ([offline LaTeX](offline-latex-editor.md#what-is-the-built-in-engine)).

## Can I use both?

Yes. Download the source from Overleaf, work on a copy in Manang, and keep the Overleaf project until you trust the result. Compile once in each and compare the PDFs: your local TeX Live may have different package versions from the one Overleaf uses. Overleaf documents [syncing with Git, GitHub and Dropbox](https://docs.overleaf.com/writing-and-editing/working-offline) for working in your own editor; those are premium features there.

## Sources

Claims about Overleaf come from Overleaf's own pages. Claims about Manang come from its code and the other pages in this guide.

- [https://www.overleaf.com/about](https://www.overleaf.com/about): Overleaf describes itself as an online LaTeX editor with real-time collaboration.
- [https://docs.overleaf.com/](https://docs.overleaf.com/): documentation index, including plan limits and integrations.
- [https://docs.overleaf.com/getting-started/recompiling-your-project/selecting-a-tex-live-version-and-latex-compiler](https://docs.overleaf.com/getting-started/recompiling-your-project/selecting-a-tex-live-version-and-latex-compiler): compilers and TeX Live versions; no mention of Typst.
- [https://docs.overleaf.com/getting-started/recompiling-your-project](https://docs.overleaf.com/getting-started/recompiling-your-project): recompiling; does not say where compiling runs.
- [https://docs.overleaf.com/collaborating/collaborating-in-overleaf](https://docs.overleaf.com/collaborating/collaborating-in-overleaf): real-time editing, sharing, track changes, commenting and chat; edits go to the server every few seconds.
- [https://docs.overleaf.com/collaborating/track-changes](https://docs.overleaf.com/collaborating/track-changes): track changes is a premium feature.
- [https://docs.overleaf.com/writing-and-editing/history-and-versioning](https://docs.overleaf.com/writing-and-editing/history-and-versioning): labels, compare, restore; full project history is premium.
- [https://docs.overleaf.com/integrations-and-add-ons/git-integration-and-github-synchronization](https://docs.overleaf.com/integrations-and-add-ons/git-integration-and-github-synchronization): Git and GitHub sync are premium features on Overleaf Cloud.
- [https://docs.overleaf.com/writing-and-editing/working-offline](https://docs.overleaf.com/writing-and-editing/working-offline): offline behaviour of the hosted editor; Git, GitHub, Dropbox and ZIP options.
- [https://docs.overleaf.com/integrations-and-add-ons/reference-manager-integrations](https://docs.overleaf.com/integrations-and-add-ons/reference-manager-integrations): ReadCube, Zotero and Mendeley integrations are premium.
- [https://docs.overleaf.com/integrations-and-add-ons/ai-features](https://docs.overleaf.com/integrations-and-add-ons/ai-features): AI features, daily allowance by plan, how to turn them off.
- [https://github.com/overleaf/overleaf](https://github.com/overleaf/overleaf): open source repository under AGPL v3; self-hosting with the Overleaf Toolkit.
- [https://docs.overleaf.com/on-premises/welcome/server-pro-vs.-community-edition](https://docs.overleaf.com/on-premises/welcome/server-pro-vs.-community-edition): self-hosted editions differ in features such as commenting and Git integration.
- [https://www.overleaf.com/user/subscription/plans](https://www.overleaf.com/user/subscription/plans): where to check current plans.

Last checked: October 2026.
