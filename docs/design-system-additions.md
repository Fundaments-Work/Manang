# Design system additions

What Manang uses that the Fundaments.work brand page (https://fundaments.work/brand/) does not define. The brand page is the authority: each item below is either to be added there or is a documented exemption. Source of truth in the app: `src/app.css`.

| # | Addition | Value | Rule |
| --- | --- | --- | --- |
| 1 | Light theme | bg `#faf9f7`, surface `#f4f3f0`, line `#d4d3ce`, text `#1c1c1c`, text-60 `#6b6b6b`; accent is lime mixed 35% toward ink; math is ink | Dark stays the base. Lime fails on paper (1.24:1), so the light accent is darkened to reach 4.5:1 for text and 3:1 for components (ADR 0002). Sage is unusable on paper, so mathematics uses ink with a hairline. |
| 2 | Extra tonal steps | `surface-hi` `#28282c`, `surface-muted` `#353539`, `recessed` `#0d0f0a`, `line-subtle` `#323236`, `text-60` `#9a9a9a`, `text-muted` `#7a7a7a` | Depth is tonal only. `text-muted` is never body text: decorative rules and disabled states. |
| 3 | Paper | `#faf9f7` page, `#1c1c1c` ink, `#6b6b6b` secondary ink | A rendered page is identical in every theme, so the screen matches print. |
| 4 | Problem markers | error: solid dot in text colour; warning: hairline ring in text-60 | No red or amber. Shape, not hue, carries severity. Always paired with a text message. |
| 5 | Translucent tints | lime at 20-45% (selection, search, PDF highlights); sage at 22% (matching brackets) | Mixes of existing tokens (`color-mix`), never new hues. |
| 6 | Collaborator cursors | six mixes of sage, text and line | No new hues. Each cursor also carries the person's name label. Needs sign-off, because the system has one accent. |
| 7 | Motion | ease `cubic-bezier(0.22, 1, 0.36, 1)`; 150 / 220 / 320 ms | Only in response to an action. Removed by `prefers-reduced-motion` and the Reduce motion setting. |
| 8 | Focus ring | 1px lime outline, 1px offset (inset on dense lists) | Visible on every control. |
| 9 | Tracking | display -0.03em, headline -0.02em, body -0.01em | Sizes and colour carry hierarchy, never a third weight. |

## Exemptions

* **User content and colour pickers.** A figure's strokes and fills are the author's choice, and the colour picker draws a hue gradient. This is content, not interface chrome, so it may use any colour.
* **Theme presets** in settings (mock and sample data) show other palettes as previews only.

## Fixed in the app

* `TemplateCard.svelte` used an arbitrary `#18181b` and a non-existent `bg-base` class. It now uses `--mng-bg` / `bg-bg`.
