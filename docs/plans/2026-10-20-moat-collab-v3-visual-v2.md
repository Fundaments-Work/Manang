# The moat: collaboration v3 and visual editor v2

Date: 2026-10-20. Owner direction: live collaboration and the visual editor are
the product's moat. Source: a read-only audit of the visual editor and the
collaboration code on 2026-10-20 (an agent report; items below are marked
[V] verified in code or docs by that audit, [I] inferred, [U] unverified).
Follows `2026-10-20-live-collab-v2.md` (project-wide live sessions).

## The claim we want to be able to make

"Plain files and real git; edit together live in Source or Visual; comments and
suggestions in the margin; roles; offline." It is defensible only after
packages I1, A1-A3 and C1 below are done and verified. Today it is not.

## Facts that shape the plan

* Collaboration v2 is on `feat/live-collab-v2`, **not on `main`**; P7 (fuzzing,
  relay conformance, ADR 0018) is still open. [V]
* `manang-live` has no identity layer: no signing, roles or members. A "viewer"
  is a UI courtesy until identity exists. [V]
* There is no annotations crate and no comments code. The git-file design
  (`.manang/comments/*.toml`) exists only on paper. [V]
* Visual editor: Markdown and Typst work (headings, emphasis, code, lists,
  links, images, quotes, math, rules, front matter). **Tables are opaque in
  Markdown and absent in Typst; citations are not rendered; LaTeX and `.docx`
  have nothing; `visual.outline` has no UI.** [V]
* The visual editor inside a live session is designed to work (it is only text)
  but no test runs both together. [U]
* Figures are last-writer-wins blobs in live sessions until the figure CRDT. [V]

## Work packages

Heavy = Rust builds of `manang-live`, `manang-live-plugin`, the format adapters
or the Tauri crate. **At most one heavy agent at a time, always capped
(`CARGO_BUILD_JOBS=4`, `nice`, `ionice`, memory cap), and no Chromium run beside
a heavy build.**

| WP | Goal | Depends | Size | Heavy |
| :--- | :--- | :--- | :--- | :--- |
| W-1 | Land collab v2: finish P7, run the full gate once alone, merge to `main` locally; the owner decides when to push | P7 | M | yes (lead, alone) |
| C1 | Visual and live together: end-to-end test, per-user undo in Visual, remote carets over hidden markers, Source and Visual users in one session | W-1 | S | no (one Chromium, alone) |
| V1 | Outline UI (headings navigator) and a Visual error surface | none | S | no |
| I1 | Identity and roles: per-member Ed25519 keys, signed membership log, viewer / commenter / editor enforced at accept time, invite carries a role capability, members in the Share popover | W-1 | L | yes |
| A1 | Annotation model: threads and suggestions as a signed side doc, anchors from Loro cursors (verify the API on docs.rs first), export to `.manang/comments/` for git | I1 (model can start earlier) | L | yes |
| A2 | Comments UI: margin gutter, thread popover, add on selection in Source and Visual, role-aware | A1 contract (mock first) | M | no |
| A3 | Suggestions and track-changes view: suggest mode, accept/reject, author colour, Visual marks (additive span kinds) | A1, A2 | L | yes then light |
| V2 | Citation and cross-reference pickers in the Visual toolbar (fills the reserved `cite` kind) | none | M | yes |
| V3 | Tables: cell model in both adapters, cell navigation, row/column edits | none | L | yes |
| V4 | LaTeX Visual adapter (hand parser, unknown macros stay opaque chips) | V2 helps | L | yes |
| V5 | Accessibility and large-document pass, 5 MB benchmark | none | M | no (plus one bench) |
| L2 | Figures as a Loro map (live figure co-editing), then version history and named versions | W-1, I1 | L | yes |

Order: W-1 (heavy) with V1 (light, no browser) alongside; then C1; then I1 with
the A2 UI against a mock; then A1, A3, V2, V3, V4, L2; V5 fills gaps. First
visible milestone after A1+A2: comments in the margin, with roles, in one live
session.

## Decisions (recommended defaults; the owner confirms D2-D5)

* D1 Order of visual work: citations (V2), tables (V3), then LaTeX (V4).
  LaTeX is the larger audience, so this stays open for the owner.
* D2 Where comments live: a signed side doc for live sessions with an export to
  `.manang/comments/` so they survive `git clone`.
* D3 Roles: a shared room key with signed roles is honest only as "accept-time
  enforcement". Real revocation needs epoch keys, which are not in this plan.
  Until then "remove member" must not be presented as secure.
* D4 Durable rooms: the stateless relay means a room dies when everyone leaves,
  so a comment from someone who left has nowhere to live. Decide before A1 ships.
* D5 `.docx`: not a moat and in tension with "files are the truth". Recommended:
  import and export only, later, or drop.
* D6 Merge `feat/live-collab-v2` to `main` before anything above (recommended;
  pushing stays the owner's call).
* D7 Do not market the moat sentence until I1, A1-A3 and C1 are verified.

## Risks

Every new package builds on code that is not yet on `main`. Loro cursor, marks
and shallow-snapshot APIs are unverified and could stall A1 and L2. A viewer who
holds the room key can read everything, and a malicious host can lie about
history. Suggestions stored in the file would pollute git; stored in a side doc
they vanish on a plain clone unless exported. Hidden markers versus screen
readers and remote carets is unverified. LaTeX Visual will always be partial.
About ten heavy slots must run one after another.
