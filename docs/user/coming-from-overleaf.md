# Coming from Overleaf

Moving a LaTeX or Typst paper from Overleaf to Manang, an offline desktop editor: what carries over, how it compiles, and where Overleaf still fits better.

Manang is a desktop app that edits plain files in a folder on your own disk. It is in public beta. This page is for people who write in Overleaf today and want to know, without marketing, what changes if they try Manang for a paper or thesis.

## Why do people move from Overleaf to Manang?

- **It works offline.** Writing, the live preview and PDF export need no network. Typst compiles with an engine built into Manang. LaTeX compiles with a TeX distribution on your computer (see [How does LaTeX compile in Manang?](#how-does-latex-compile-in-manang)), with one online step in one setup described there.
- **Your files are plain text on disk.** A project is an ordinary folder of UTF-8 files. You can open it with any other editor, back it up like any folder, or put it in Git. See [Projects and Files](projects-and-files.md).
- **No account.** Manang has no sign-in. Nothing needs a login to write, compile or save versions.
- **One app for the whole paper.** The editor, preview, PDF viewer, references panel, figure studio and version history are in the same window.

## Can I open my Overleaf project in Manang?

Yes, as long as you can get the source files out of Overleaf. Manang opens a folder, not an Overleaf link, and it does not connect to Overleaf.

One thing to know first: Manang identifies a project by a small file called `manang.toml` in the project folder. In the code that opens a project (checked in this release), a folder without `manang.toml` is refused with the message "not a Manang project (no manang.toml)". An Overleaf download does not contain this file, so you add it yourself. It takes one minute.

### Bring a LaTeX project over

1. In Overleaf, download the project source as a ZIP. Overleaf has a download-source option in its project menu; check Overleaf's own help if the menu has changed.
2. Unzip it into a new folder on your computer.
3. In that folder, create a text file named `manang.toml` with this content, changing `main` to the name of your root `.tex` file:

   ```toml
   [project]
   engine = "latex"
   name = "My paper"
   main = "main.tex"
   ```

   `engine` is required. `name` is optional. `main` is optional and defaults to `main.tex`; if you set it, it must end in `.tex` for a LaTeX project. Unknown keys are reported as errors with a hint rather than ignored, so a typo does not silently change behaviour. See [Supported File Types](file-types.md#7-the-manangtoml-project-file).
4. In Manang, choose **File > Open folder…** and select the folder.
5. If your root file is not called `main.tex`, right-click it in the Files panel and choose **Set as main file**. Manang saves the choice as `main` in `manang.toml` and compiles that file whichever tab you are looking at.
6. Compile with `Ctrl+Shift+B` (`Cmd+Shift+B`), or just wait: with **Auto compile** on, Manang rebuilds after a short pause as you type.

### What happens to each kind of file

- **`.tex`, `.sty`, `.cls`, `.dtx`, `.ltx`:** open in the text editor with LaTeX highlighting. `.tex` files are compiled; the others are included from them. Multi-file projects with `\input` or `\include` stay as they are, because Manang does not rewrite your files. See [Supported File Types](file-types.md).
- **`.bib`:** opens in the editor and is read by the References panel. You may need to click **Reload bibliography files** there if you changed it outside the app. See [References and bibliographies](#references-and-bibliographies).
- **Images:** `.png`, `.jpg`, `.jpeg`, `.gif`, `.webp`, `.svg`, `.bmp`, `.avif` and `.ico` open in a viewer tab. Your `\includegraphics` lines are not changed. PDF figures open in the built-in PDF viewer.
- **`manang.toml`:** the only file Manang adds. Live whole-project sessions do not send it to other people.
- **Anything else:** text files open as text, and binary files open in an information tab with an **Open with system app** button.

Because the files are not converted, what you get is what Overleaf exported. The result can still differ from what Overleaf produced if your local TeX distribution has different package versions than the one Overleaf used. Compile once and compare the PDF with the version you already have before you rely on it.

### Bring a Typst project over

The steps are the same, with these differences:

1. Use `engine = "typst"` and a `main` that ends in `.typ` (the default is `main.typ`).
2. Typst compiles inside Manang with no extra installation.
3. Open the folder with **File > Open folder…** and use **Set as main file** if needed.

### Start fresh instead

If you only want to try the editor first, create a project from a template and paste your text in. See [Templates](templates.md). The LaTeX templates are a plain article, a blank starter and a lab report. They are not journal-specific, so keep your journal's own class files in the project folder.

## How does LaTeX compile in Manang?

Manang does not ship a TeX distribution. It uses one of two routes.

1. **Your own TeX installation.** Manang uses TeX Live, MiKTeX or MacTeX when it finds one on your `PATH`. You choose `pdflatex`, `xelatex` or `lualatex` under **Settings > Plugins > LaTeX**. This works on Windows, macOS and Linux and handles whatever your TeX installation handles. Setup instructions are in [Preview and PDF](preview-and-pdf.md#6-install-tex-live-for-latex).
2. **The optional built-in engine.** If no TeX is found, Manang offers **Download the built-in engine (about 48 MB)**. Nothing is downloaded until you agree. In this beta it exists only for **Linux on x86-64 and arm64**; Windows and macOS builds are planned, so on those systems you install TeX Live, MiKTeX or MacTeX instead.

The built-in engine has limits you should know before you move a complex project:

- It is XeTeX (Tectonic). It handles most articles, theses and Beamer slides, but it does not run packages that only work with `pdflatex`, it does not run LuaLaTeX, and it has no `biber`. BibTeX works.
- The first time a document uses a LaTeX package, the engine fetches that package from the internet and keeps it on your computer. After that, compiling needs no network. You can turn the fetching off under **Settings > LaTeX** to stay fully offline, but then packages you have never used are reported as not found.
- In the default **Automatic** setting, your own TeX comes first and the built-in engine is only used when there is none.

If your Overleaf project depends on `biber`, LuaLaTeX or packages that need `pdflatex`, plan on installing a full TeX distribution.

When compilation fails, errors appear in the **Problems** tab, and small ones such as a missing `\usepackage` get a **Fix** button; see [Fixing Errors](fixing-errors.md). You can jump between source and PDF in both directions with SyncTeX; see [Preview and PDF](preview-and-pdf.md#3-bidirectional-synctex-navigation).

## Is Typst a good alternative to LaTeX?

Typst is a separate, newer markup language and typesetting system with its own syntax, not a LaTeX dialect. A LaTeX document does not convert to Typst automatically; Manang has a Markdown-to-Typst converter only (see [Writing](writing.md)). Moving a thesis from LaTeX to Typst means rewriting it.

What Typst gives you in Manang: the compiler is built in, so there is nothing else to install and it works offline from the first launch. What it costs you: journals and publishers that provide LaTeX classes will normally not provide Typst ones, so for a submission that must use a journal's `.cls` file, LaTeX is usually the practical choice. Manang edits both and keeps them side by side, so you can write one paper in each. See [Writing](writing.md) for both languages and [The Visual Editor](visual-editor.md) for formatted editing of Typst and Markdown.

## References and bibliographies

Manang reads `.bib` and `.biblatex` files (BibTeX and BibLaTeX) and CSL-JSON (`.json`, `.csl`) in your project and lists them in the **References** panel. You can filter by key, title, author or year, and press **Insert** to put `\cite{key}` (LaTeX), `@key` (Typst) or `[@key]` (Markdown) at the cursor.

Your existing bibliography commands keep working because Manang does not change them:

```latex
\bibliographystyle{ieeetr}
\bibliography{references}
```

or, with `biblatex`:

```latex
\usepackage[style=apa]{biblatex}
\addbibresource{references.bib}
```

Remember that `biblatex` with the `biber` backend needs a TeX installation that includes `biber`; the built-in engine does not. The **Verify** button on a reference checks your entry against Crossref, OpenAlex or arXiv. This is the one part of the References panel that uses the network, and only when you press it. See [References and Bibliographies](references.md) for details.

## How does collaboration differ from Overleaf?

This is the biggest difference, so here it is plainly: **Manang is not a drop-in replacement for Overleaf's real-time collaboration.** It has three ways of working with other people, and none of them is the same thing as a shared online project.

- **Live editing ([Edit Together Live](edit-together-live.md)).** You can share one file or a whole project. Co-authors see each other's carets and edits as they type. The text is end-to-end encrypted, and the relay in the middle (by default `wss://relay.fundaments.work`, or one you run yourself) cannot read it. Limits that matter for a team:
  - There are no accounts and no roles. Everyone with the invite has the same rights and can change files in your project folder. Nobody can be set to read-only or removed; to stop sharing with someone you leave and start a new session.
  - A session needs at least one person in it. When everyone leaves, it is gone. It is not a project that stays online for the others to visit later.
  - In a whole-project session, text files merge edit by edit, but images, figures and other non-text files are last writer wins.
  - Co-authors need Manang installed and open at the same time.
- **Share and get updates ([Share and Get Updates](share-and-get-updates.md)).** This is Git-based and turn-based. You connect a repository address from GitHub, GitLab or your own server, share your saved versions, and press **Get updates** to bring in theirs. If you both changed the same lines, Manang stops before touching your files and tells you which ones. Sign-in uses the git credentials or ssh keys already on your computer.
- **Anything else.** Because a project is a folder of plain files, you can also use Git yourself, or a shared folder.

What Manang does not have, as of this beta: **track changes, review comments, or a chat inside the project**. There is no feature that shows each co-author's suggestions inline for you to accept or reject. The [AI assistant](ai-assistant.md) proposes edits as a diff you accept or reject, but that is separate from collaboration. If your supervisor reviews by leaving comments in the document, check how that would work before you move.

## How does version history compare?

Manang's **Save a version** (`Ctrl+Alt+S`, `Cmd+Option+S`) records the state of your whole project under a name you choose. Versions are stored with standard Git inside the project, so you need no terminal and no account, and any Git tool can read them. You can browse the timeline, see line-by-line diffs, and restore one file or the whole project without losing later versions. See [Version History and Restore](version-history.md).

The model is explicit saves of named versions, not a continuous record of every edit. Manang does keep recovery snapshots of unsaved work after a crash and safety copies before destructive actions; see [Crash Recovery and Safety Copies](crash-recovery.md). Check how often you rely on a fine-grained edit history before you decide whether named versions are enough.

## When should I stay on Overleaf?

Manang is a good fit for people who write mostly alone or with a few co-authors, want files on their own disk, and are happy to use a local TeX installation or Typst. Staying on Overleaf, or using both, is the better choice in these cases:

- **A larger team edits the same project in real time** and needs accounts, read-only access, review comments and track changes. Manang does not offer these.
- **You rely on an Overleaf-only template, a journal's submission integration or another Overleaf-hosted workflow.** Manang does not connect to Overleaf or to publishers, so you would export the files and submit them yourself.
- **You do not want to install anything for LaTeX.** On Windows and macOS, Manang needs a TeX distribution that you install. Only on Linux does it offer a built-in engine, and that engine has the limits above.
- **Your project needs `biber`, LuaLaTeX or `pdflatex`-only packages and you do not want to set up a full TeX distribution.**
- **Co-authors cannot or will not install software.** A web browser works for them in Overleaf; Manang requires a desktop installation. Manang is also a beta, and its installers are not code-signed yet (see [Install Manang](install.md)).

Nothing stops you from trying Manang on a copy. Download the source ZIP, follow the steps above, and keep the Overleaf project as it is until you are sure.

## Next steps

1. [Install Manang](install.md), and a TeX distribution if you write LaTeX on Windows or macOS.
2. Follow [Getting Started](getting-started.md) to learn the window.
3. Read [Preview and PDF](preview-and-pdf.md) for the compile settings.
