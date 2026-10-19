# SEO and content plan for Fundaments.work, Manang, Theorem and Lemma

Written 2026-10-19. Goal: people who search for what our tools do find them, and
trust what they find. We do this with accurate pages that answer real queries, a
technically clean site, and listings on the places our users already look. We do
not buy links, publish thin pages, or use analytics that track visitors (our own
promise: zero telemetry).

## 1. The sites and what each should rank for

| Property | Address | Job | Queries it should win |
| :--- | :--- | :--- | :--- |
| Fundaments.work | `fundaments.work` | Brand hub, links to every product, hosts the docs | "Fundaments.work", "local-first software studio" |
| Docs | `fundaments.work/docs/<product>` | Long-form answers, how-to, reference | the how-to and "how do I" queries below |
| Manang | `manang.fundaments.work` | Download and convince | "Overleaf alternative", "offline LaTeX editor", "Typst editor", "LaTeX editor Linux" |
| Theorem | `theorem.fundaments.work` | Download and convince | "offline EPUB reader Linux", "PDF highlights to Markdown", "Obsidian reading highlights" |
| Lemma | `lemma.fundaments.work` (new, one page) | Convince, then send to Obsidian | "Obsidian spaced repetition plugin", "Anki alternative Obsidian", "FSRS Obsidian" |

Names are single-level under `fundaments.work` (the certificate covers one level).

## 2. Technical foundation (all sites, do once, week 1)

1. **Search Console and Bing Webmaster Tools**, as domain properties verified by DNS
   (Cloudflare). Submit each sitemap. This is the only measurement we need.
2. **Canonical homes.** Docs are canonical at `fundaments.work/docs/<product>`; each
   product site links to its docs prominently (header, hero, footer). Every page has one
   `<link rel="canonical">`, a unique title (50 to 60 characters) and description
   (140 to 160), and one `h1`.
3. **Structured data (JSON-LD):** `Organization` (with `sameAs` to GitHub) on the hub;
   `SoftwareApplication` on each product page (name, `operatingSystem`,
   `applicationCategory`, `offers` price 0, `downloadUrl`, `softwareVersion` read from the
   latest release at build time, `license`); `FAQPage` where a real FAQ exists;
   `BreadcrumbList` and `TechArticle` on docs pages.
4. **Social cards:** a 1200x630 image per page type, generated from the design system, so
   links look right in chat, Mastodon, Reddit and Hacker News.
5. **Performance:** the sites are static; keep each page under about 200 KB before images,
   serve WebP/AVIF with width and height set, self-host fonts (done). Targets: LCP under
   2.0 s, CLS under 0.05 on a mid phone. Check with Lighthouse in CI once a month, not on
   every push.
6. **Crawl hygiene:** `robots.txt` allowing everything and naming the sitemap; sitemap
   lists only canonical pages; real `404`; redirects are 301 (the old `/brand/` goes to
   `/docs/design-system/`); `llm.txt` (exists) kept in sync with real content.
7. **No tracking.** Use Search Console, GitHub traffic and star history, release download
   counts and Cloudflare's edge analytics (server side, no script, no cookies). Do not add
   a JavaScript analytics tag.
8. **GitHub is a search engine too.** Each repo: a one-sentence description with the main
   keywords, 15 to 20 topics, a social preview image, a README that starts with what it
   is and an install command, a LICENSE, releases with real notes.

## 3. Content to write, by property

Rule for every page: it must be true of the current build, show the real app (replace the
browser sample screenshots with desktop captures), and answer one query completely. No
filler, no pages made only to hold a keyword.

### Manang (highest value: large, searched, competitive)

1. **Comparison pages** (honest, with a table and when the other tool is better):
   Manang vs Overleaf; vs TeXstudio; vs VS Code with LaTeX Workshop; vs the Typst web app.
2. **Guides** (docs, how-to): write a thesis in Typst; move from LaTeX to Typst; draw
   TikZ and CeTZ figures visually; edit a paper together without a cloud account; compile
   LaTeX offline on Linux; SyncTeX click-to-jump explained; use Manang with Git.
3. **Template gallery:** one page per built-in template (IEEE-style paper, thesis, slides,
   CV, lecture notes). Each page has a preview image, the source, and a download. These
   catch long-tail queries like "Typst thesis template".
4. **Use-case pages** on the product site: thesis, journal paper, lecture notes, slides.
5. **Release pages:** every GitHub release and the changelog stay indexable; the changelog
   page lists versions with dates.
6. **Distribution listings** (high-authority backlinks and discovery): Flathub, AUR
   (`manang-bin`), winget, Homebrew cask, Scoop; AlternativeTo and Product Hunt; Show HN;
   r/typst, r/LaTeX, r/rust, r/linux; Typst Discord and forum; awesome-typst lists.

### Theorem

1. Pages per reader need: highlights to Markdown for Obsidian; for Logseq; offline EPUB
   reader for Linux; PDF annotation with a pen; read aloud offline.
2. Format pages (EPUB, MOBI, FB2, CBZ, PDF) with exactly what is supported.
3. Comparisons: vs Calibre viewer, KOReader, Zotero reader, Readwise Reader (privacy and
   offline are the real differences).
4. Listings: Flathub, AUR, winget, Obsidian forum and subreddit, AlternativeTo.

### Lemma (new one-page site plus docs)

1. One page at `lemma.fundaments.work`, from the Theorem template: what it does, a short
   demo clip, install through the Obsidian community plugin directory, FAQ, link to docs.
2. Docs: card syntax, FSRS-5 explained plainly, importing from Anki, syncing with your
   own CouchDB, mobile notes.
3. Comparison: vs the Obsidian Spaced Repetition plugin and vs Anki.
4. Check the plugin is listed in the Obsidian community plugin directory; that listing and
   the Obsidian forum matter more than any page of ours.

### Fundaments.work hub

Product rows with logos (see section 5), an About page that states who we are and the
promises, the docs home, and links to every repo. Targets the brand query and passes
authority to the products.

## 4. Order of work

| When | What |
| :--- | :--- |
| Week 1 | Section 2 in full; hub logos; Manang big wordmark; Lemma one-pager; verify all sitemaps |
| Weeks 2 to 4 | Manang: three comparison pages, four guides, template gallery with real previews; Flathub and AUR submissions |
| Weeks 5 to 8 | Theorem: the page set above; Lemma docs; winget, Homebrew, Scoop; Show HN and Reddit posts timed with a release that has real news |
| Weeks 9 to 12 | Second round of guides from Search Console queries that already show impressions; refresh comparisons; screenshots and clips |

Review monthly in Search Console: queries with impressions but no clicks get a better
title and answer; pages with no impressions after 60 days are merged or removed.

## 5. Brand and design items this depends on

- Product marks: Manang has no logo yet (only a favicon); Theorem and Lemma have marks.
  Draw Manang's mark under the design system's product-mark rules, then show all three
  at one size beside the names on the hub.
- Manang's landing page needs its oversized wordmark and a larger headline, like the hub
  and Theorem.
- Real desktop screenshots in place of the browser sample captures.

## 6. Risks

- **Thin or generated pages** hurt more than they help. Every page here is tied to a real
  feature and reviewed by a person who uses the tool.
- **Claims that are not true yet** (signed installers, built-in LaTeX engine on every
  system) must not appear in titles or structured data.
- **Split authority** between the product sites and the docs: link between them often and
  keep one canonical home per topic.
