# References and Bibliographies

Load BibTeX and CSL-JSON bibliographies, search entries, insert citations at the cursor, choose a citation style, and verify references against Crossref.

## 1. Set up bibliography files

Manang discovers and indexes bibliography files in your project directory automatically.

1. Place your bibliography files in your project root or a subfolder:
   - **BibTeX / BibLaTeX:** `.bib` or `.biblatex` files.
   - **CSL-JSON:** `.json` or `.csl` files.
2. Click the **References** icon in the Activity bar (or run `Show references` from `Ctrl+K` (`Cmd+K`)).
3. Manang indexes all entries in memory. If you add or edit a `.bib` file outside the app, click the **Reload bibliography files** icon button in the References panel header to update the index.

## 2. Choose a citation style

You can set how citations appear in your compiled documents and in reference previews.

### Set the preview formatting style
1. Open Settings (`Ctrl+,` (`Cmd+,`)).
2. Under **Plugins**, open the **Citations** section.
3. Choose the default **Citation style** (`citations.style`):
   - `apa`: Author-year format (for example: *Turing, A. M. (1936). On computable numbers...*).
   - `numeric`: Numbered IEEE-like format (for example: *[1] A. M. Turing, "On computable numbers..."*).
4. Select whether this applies globally (User) or for the active project only (This project).

### Set the bibliography style in documents
Configure the bibliography command in your document source:
1. **Typst:**
   ```typst
   #bibliography("references.bib", style: "apa")
   ```
   Other supported Typst styles include `"ieee"`, `"chicago-author-date"`, `"mla"`, and `"nature"`.
2. **LaTeX:**
   When using standard BibTeX:
   ```latex
   \bibliographystyle{ieeetr}
   \bibliography{references}
   ```
   Or when using `biblatex`:
   ```latex
   \usepackage[style=apa]{biblatex}
   \addbibresource{references.bib}
   ```
3. **Markdown:**
   Specify a Citation Style Language (CSL) file in your document frontmatter:
   ```yaml
   ---
   bibliography: references.bib
   csl: ieee.csl
   ---
   ```

## 3. Search the bibliography

1. Open the **References** sidebar panel from the Activity bar.
2. Type in the **Filter by author, title or key** field at the top of the panel.
3. Manang ranks entries matching:
   - Citation keys (e.g. `shannon1948`), including partial matches
   - Words in the article or book title
   - Author family and given names
   - Publication year

## 4. Insert citations into a document

1. Open the document where you want to cite a source.
2. Place the text editor cursor at the insertion point.
3. In the References panel, locate the entry.
4. Click **Insert** on the reference card.
5. Manang inserts the correct citation syntax for your document engine:
   - **Typst:** `@key`
   - **LaTeX:** `\cite{key}`
   - **Markdown:** `[@key]`
6. When compiling Typst or LaTeX, citations automatically resolve to the bibliography list at the end of the document.

## 5. Verify references online

Manang includes a citation verification engine that checks your local bibliographic metadata against authoritative online registries.

1. Open the **References** panel. Each card shows the authors, year, title, venue, citation key and entry type.
2. To see more, click the arrow button on the card to expand its details: all authors, the DOI link, URL, arXiv identifier, and abstract.
3. Click the **Verify** button on the reference card.
4. Manang queries Crossref, OpenAlex, or arXiv over a secure, sandboxed connection.
5. Review the verification badge on the card:
   - **Verified:** Your local citation matches the authoritative record.
   - **Mismatch (amber):** Differences exist between your file and the registry (such as corrected author spelling, page ranges, or volume numbers). The card lists the metadata differences.
   - **Not found:** No matching record was found in the online registries.
   - **Error (red):** The network request failed; the card shows the message.
6. To copy a pre-formatted citation string, expand the card. Under **Formatted citation** you see the citation in the style chosen in Settings (the style button opens Settings, **Citations**); click the copy button beside it.
