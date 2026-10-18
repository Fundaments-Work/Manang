# References and Bibliographies

You can manage bibliography files, search citations, choose citation styles, insert reference keys at the cursor, and verify metadata against authoritative online registries.

## 1. Set up bibliography files

Manang discovers and indexes bibliography files in your project directory automatically.

1. Place your bibliography files in your project root or a subfolder:
   - **BibTeX / BibLaTeX:** `.bib` or `.biblatex` files.
   - **CSL-JSON:** `.json` or `.csl` files.
2. Click the **References** icon in the Activity bar (or run `Show references` from `Ctrl+K` (`Cmd+K`)).
3. Manang indexes all entries in memory. If you add or edit a `.bib` file outside the app, click the **Reload references** icon button in the References panel header to update the index.

## 2. Choose a citation style

You can set how citations appear in your compiled documents and in reference previews.

### Set the preview formatting style
1. Open Settings (`Ctrl+,` (`Cmd+,`)).
2. Scroll to **Citations**.
3. Choose the default **Citation style** (`citations.style`):
   - `apa`: Author-year format (for example: *Turing, A. M. (1936). On computable numbers...*).
   - `numeric`: Numbered IEEE-like format (for example: *[1] A. M. Turing, "On computable numbers..."*).
4. Select whether this applies globally (User scope) or for the active project only (Project scope).

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
2. Type in the search field at the top of the panel.
3. Manang ranks entries matching:
   - Citation keys (e.g. `shannon1948`)
   - Words in the article or book title
   - Author family and given names
   - Publication year
   - Journal or conference venue
   - Digital Object Identifier (DOI)

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

1. Open the **References** panel and click any reference card to expand its details.
2. View the entry's authors, year, publication venue, DOI link, arXiv identifier, and abstract.
3. Click the **Verify** button on the reference card.
4. Manang queries Crossref, OpenAlex, or arXiv over a secure, sandboxed connection.
5. Review the verification badge on the card:
   - **Verified (green):** Your local citation matches the authoritative record in Crossref or OpenAlex.
   - **Mismatch (amber):** Differences exist between your file and the registry (such as corrected author spelling, page ranges, or volume numbers). Click to review the difference table.
   - **Not found / Error (red):** The DOI or arXiv identifier could not be resolved or the network request failed.
6. To copy a pre-formatted citation string, select **APA** or **IEEE** in the entry card's format dropdown and click **Copy**.
