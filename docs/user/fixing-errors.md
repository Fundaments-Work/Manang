# Fixing errors

Small, mechanical compile errors have one obvious repair: a forgotten `\usepackage`, a Typst package you never imported, a `}` left open. Manang offers that repair as a **Fix** button. It never guesses between candidates and never uses the network or an AI model. Anything it does not recognise stays with **Fix with AI**.

## 1. Where Fix appears

- **Problems panel:** a **Fix** button sits next to **Fix with AI** on each error that has a fix. When an error has several fixes the button reads **Fix…** and opens a small list.
- **Editor:** hover the marked text and the callout shows **Quick fix: …** buttons. With the caret on the error, press `Ctrl+.` (`Cmd+.` on macOS). If there are several fixes a list opens; use the arrow keys, `Enter` and `Escape`.
- **Fix all:** when a file has two or more certain fixes, its group in Problems shows **Fix all (n)**. It applies them as one edit, so one `Ctrl+Z` undoes all of it.

The button's tooltip says why the fix applies. Nothing is applied until you press it.

## 2. What happens after a fix

The status bar says what changed, for example *Added \usepackage{graphicx}.*, with an **Undo** link. The document then recompiles. If the same error is still there and there are no fewer errors, the notice says so and offers **Undo** again.

For LaTeX, which compiles the saved file, Manang saves the document after a fix only when it had no other unsaved edits. Otherwise the notice says *Save to compile*.

## 3. What it fixes

Certain fixes have exactly one reading and can be applied in bulk. Likely fixes choose the most plausible place or reading, so you decide each one.

| Engine | Error | Fix | Kind |
| --- | --- | --- | --- |
| LaTeX | undefined command such as `\includegraphics`, `\textcolor`, `\toprule` | add `\usepackage{graphicx}` (and so on) after the last `\usepackage`, else after `\documentclass` | certain |
| LaTeX | command two packages provide, such as `\url` | one fix per package | likely |
| LaTeX | environment from a missing package (`align`, `tikzpicture`) | add the package | certain |
| LaTeX | TikZ feature without its library (`right=of a`, `-{Stealth}`) | add `\usetikzlibrary{positioning,arrows.meta}` | certain or likely |
| LaTeX | `theorem` never declared | add `amsthm` and `\newtheorem` | likely |
| LaTeX | missing `\end{document}` | add it at the end | certain |
| LaTeX | wrong or missing `\end{...}`, one stray `{` or `}` | rename, add or remove it | likely |
| Typst | unknown name from a known `@preview` package (`cetz`, `fletcher`, `touying`, `physica`, ...) | add or extend an `#import` | certain |
| Typst | one-letter slip of a keyword or known function (`#sett`) | correct the spelling | likely |
| Typst | unclosed `(`, `[`, `{`, `$`, string, raw block | add the closer | likely |
| Markdown | code fence never closed | close it at the end | certain |
| Markdown | table delimiter row narrower than the header | widen it | likely |

A package that is already loaded is never added twice, and a file with no `\documentclass` (an included chapter) gets no package fix.

## 4. Settings

Open Settings and look under **Self-healing**:

- **Offer one-click fixes** (`heal.enabled`, on): turn all fixes off.
- **Apply certain fixes on save** (`heal.auto-apply`, off): when you save, certain fixes are applied and the notice says so. Likely fixes are never applied automatically.
