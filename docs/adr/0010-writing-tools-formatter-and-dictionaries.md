# ADR 0010: Writing tools - formatter, spell checker and dictionary licensing

Status: accepted (2026-10-17)
Scope: `crates/manang-writing`

## Context

The writing-tools plugin needs a Typst formatter, a spell checker with
redistributable dictionaries, math symbol tables and a lightweight prose
linter. Everything ships inside an MIT-licensed desktop application, runs
natively in Rust, and must stay under the latency budgets in `AGENTS.md`.

## Decisions

### Typst formatting: `typstyle-core` 0.15.2

`typstyle` publishes its engine as a library crate, `typstyle-core`
(Apache-2.0, depends on `typst-syntax ^0.15.1`, the same parser version
`manang-typst` pins). It supports whole-document and range formatting, which is
exactly the `format.document` / `format.range` surface. Apache-2.0 is
compatible with MIT distribution (keep the NOTICE/licence text in the
third-party licence screen).

Consequences:

* `typstyle-core` 0.15 declares `rust-version = 1.92`; `manang-writing` sets
  the same MSRV (the workspace default of 1.85 is lower than what Typst 0.15
  itself already needs in practice).
* Idempotence is a tested invariant, and is also enforced at run time: the
  wrapper re-formats the output until it is a fixpoint (at most three extra
  passes). If it never stabilises, or the output does not parse cleanly, the
  source is returned unchanged with a `skipped` reason. `format(format(x)) ==
  format(x)` therefore holds by construction.
* Compiled-meaning preservation is tested by compiling documents before and
  after formatting with `manang-typst` and comparing the rendered SVG pages,
  for fixtures and for generated documents (`tests/format_semantics.rs`).

Markdown uses a deliberately small, line-based normaliser written in the crate:
trailing whitespace, runs of blank lines, document-leading/trailing blank
lines and the final newline. It never reflows prose, never touches fenced
code, indented code, front matter or `$$` math, and keeps hard line breaks.
Re-ordering list markers or re-aligning tables were rejected because they can
change CommonMark structure (a different bullet character starts a new list).

LaTeX returns an explicit `unsupported` error. No Rust LaTeX formatter handles
verbatim environments, `\catcode` changes and macro-defined environments
safely enough to promise "never changes meaning". `tex-fmt` (MIT) is the
candidate to evaluate if a formatter is wanted later; it is a CLI first, so
embedding it needs a separate decision.

### Spell checking: `spellbook` 0.4.2

`spellbook` is a pure-Rust, no-FFI Hunspell-format checker and suggester
(the engine behind Helix's spell support). Licence: MPL-2.0. MPL-2.0 is
file-level copyleft: using it unmodified as a dependency of an MIT application
is fine; we must ship its licence text and must publish any changes made to
`spellbook`'s own files (we make none).

Rejected: `zspell` (0.5.5; no licence metadata on crates.io, less complete
Hunspell support, slower), Hunspell/Nuspell bindings (C/C++ FFI, build-time
system dependency, harder to sandbox and cross-compile).

Dictionary loading is data driven. `<config_dir>/dictionaries/` accepts
`xx_YY.aff` + `xx_YY.dic` (or `xx_YY/index.aff` + `index.dic`); the language
becomes available on the next request, without a rebuild. Files must be UTF-8.
A user dictionary file takes precedence over a bundled one with the same code.

### Bundled dictionaries and licences

| Language | Source | Version | Licence |
| --- | --- | --- | --- |
| `en_US` | SCOWL `en_US` Hunspell build, via `dictionary-en` 4.0.0 (wooorm/dictionaries) | 2020.12.07 | SCOWL licence (MIT-style permissive; package metadata `MIT AND BSD`) |
| `en_GB` | SCOWL `en_GB-ise` Hunspell build, via `dictionary-en-gb` 3.0.0 | 2020.12.07 | same |

The SCOWL licence (full text in `dict/LICENSE-SCOWL-*.txt`, shipped with the
crate) grants permission to "use, copy, modify, distribute and sell these word
lists ... for any purpose" provided the copyright notice and permission notice
appear in all copies. It aggregates Ispell (BSD), WordNet (Princeton, permissive),
public-domain Moby/12Dicts and other permissive sources. That is compatible with
MIT redistribution **provided the notice is reproduced**: the application's
third-party licence screen must include `LICENSE-SCOWL-en_US.txt` (the en_GB
file differs only in its header).

`en_GB` is the SCOWL build, not the widely packaged LibreOffice en_GB
dictionary: that one is, to our knowledge, LGPL-licensed and was therefore not used. Both bundled builds
use `-ise` British spelling.

Adding more bundled languages requires a per-language licence check; many
common Hunspell dictionaries are GPL, LGPL or MPL and must not be bundled
without review. Users can still drop any dictionary into the directory
themselves; the licence is then theirs to honour.

The `.aff`/`.dic` files are stored gzip-compressed (`flate2`, pure-Rust
`miniz_oxide` backend), about 190 KiB each, and inflated lazily on first use.

### Symbols and prose lint

The symbol table (`data/symbols.txt`) is curated by hand. Every Typst entry is
verified by tests against Typst's own symbol table (`typst` 0.15.1, built on
`codex` 0.3.0): the character must match and every insertion text must compile
in math mode. LaTeX commands come from the amsmath, amssymb, mathtools and
standard LaTeX symbol lists; the package a command needs is recorded.

Prose lint is a small rule engine instead of Vale: Vale is a Go binary with its
own style packages, which would break the single-binary, local-first,
sandboxable model and cannot be run on masked text. The rules are heuristics
(repeated words, sentence length, passive voice, weasel words, double spaces,
inconsistent hyphenation), cheap, deterministic and individually configurable.

## Consequences

* New dependencies: `typstyle-core`, `spellbook`, `flate2`, `pulldown-cmark`
  (MIT), `toml`.
* Third-party notices to surface in the UI: Apache-2.0 (typstyle), MPL-2.0
  (spellbook), SCOWL licence (dictionaries).
* Suggestions are expensive (one lookup costs roughly 2-20 ms): `spell.check` computes them for
  at most 5 distinct words per request and `spell.suggest` fetches the rest on demand;
  pass `max_suggestions: 0` for pure checking (about 2 ms for a 10k-word document).
