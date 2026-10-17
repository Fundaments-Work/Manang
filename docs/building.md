# Building Manang

## LaTeX backends

LaTeX compiles with the **system TeX installation** by default (TeX Live or
MiKTeX: `latexmk`, `pdflatex`, `xelatex`, `lualatex`, `biber` on `PATH`). The
preview header shows which engine ran. Without a TeX installation the preview
says so and links to the install pages; nothing crashes.

Tectonic is an **optional, on-demand** engine for users without TeX. It is not
bundled in the installer and there is no Tauri `externalBin` sidecar.

Tectonic statically bundles HarfBuzz; linking it into the app binary overrides
the system HarfBuzz that GTK/WebKit use and crashes the app at start-up
(AGENTS.md 7.4, ADR 0004). So the app never links it: it runs as a separate
`manang-latex-worker` process, built behind the `manang-latex` feature `engine`.

Check the app is clean (must print nothing):

```sh
cargo tree -p manang | grep -iE "tectonic|harfbuzz|mupdf|pdfium"
```

### Testing the optional worker locally

Debug builds look for the worker next to the app executable:

```sh
cargo build -p manang-latex --features engine --bin manang-latex-worker
```

Needs system `harfbuzz`, `fontconfig`, `icu`, `openssl` and `pkg-config`. Set
`MANANG_LATEX_WORKER=/path/to/worker` to use one elsewhere.
