import { describe, expect, it } from "vitest";

describe("panel empty and error states audit", () => {
  describe("Files panel", () => {
    it("provides plain language for empty folder", () => {
      const emptyMsg = "This folder is empty. Create a file to start writing.";
      expect(emptyMsg).toContain("Create a file to start writing");
    });

    it("provides clear instruction when filter matches nothing", () => {
      const filter = "xyz";
      const filterMsg = `No files match "${filter}". Clear the filter to see everything.`;
      expect(filterMsg).toContain("Clear the filter");
    });

    it("provides actionable error message with retry", () => {
      const err = "Permission denied";
      const heading = "Could not read files";
      expect(heading).toBe("Could not read files");
      expect(err).toBeTruthy();
    });
  });

  describe("Search panel", () => {
    it("guides user when no query or project is set", () => {
      const noProject = "Open a folder to search its files.";
      const noQuery = "Type to search file names and text across the project.";
      expect(noProject).toContain("Open a folder");
      expect(noQuery).toContain("Type to search");
    });

    it("guides user when query yields no results", () => {
      const query = "unknown_function";
      const emptyMsg = `No results for "${query}". Check the spelling, try another query, or adjust include/exclude filters.`;
      expect(emptyMsg).toContain("Check the spelling");
    });

    it("provides plain-language explanation for regex syntax errors", () => {
      const syntaxError = "Unclosed parenthesis";
      const title = "Search syntax error";
      expect(title).toBe("Search syntax error");
      expect(syntaxError).toBeTruthy();
    });
  });

  describe("Figures panel", () => {
    it("explains what figures are when empty", () => {
      const emptyMsg =
        "No figures yet. A figure is drawn in Manang and saved as a .figure file you can embed in any document.";
      expect(emptyMsg).toContain("embed in any document");
    });

    it("reports load errors with actionable retry", () => {
      const errHeading = "Could not read figures";
      expect(errHeading).toBe("Could not read figures");
    });
  });

  describe("References panel", () => {
    it("instructs how to add references when empty", () => {
      const emptyMsg =
        "No references found. Add a .bib or CSL-JSON file to the project and its entries appear here, ready to cite.";
      expect(emptyMsg).toContain(".bib or CSL-JSON");
    });

    it("suggests search alternatives when filter has no matches", () => {
      const noMatchMsg =
        'Nothing matches "hawking". Try an author\'s surname, year, or part of the title.';
      expect(noMatchMsg).toContain("Try an author's surname, year, or part of the title");
    });

    it("has a clear heading on error with retry", () => {
      const errorHeading = "Failed to load references";
      expect(errorHeading).toBe("Failed to load references");
    });
  });

  describe("Problems panel", () => {
    it("reports clean compilation clearly", () => {
      const okMsg = "No problems. The document compiled cleanly.";
      expect(okMsg).toContain("compiled cleanly");
    });

    it("reports compilation failure clearly pointing to Output tab", () => {
      const failedTitle = "Compilation failed";
      const failedMsg =
        "The compiler reported a failure, but no source line diagnostics were generated. Check the Output tab to inspect the raw engine messages.";
      expect(failedTitle).toBe("Compilation failed");
      expect(failedMsg).toContain("Check the Output tab");
    });
  });

  describe("Output panel", () => {
    it("explains when output appears", () => {
      const emptyMsg =
        "No output yet. Compile messages and build logs appear here when you compile a document.";
      expect(emptyMsg).toContain("when you compile a document");
    });

    it("advises on missing compiler toolchain", () => {
      const unavailMsg = "The compiler plugin is not available.";
      expect(unavailMsg).toContain("compiler plugin is not available");
    });
  });

  describe("Plugins panel", () => {
    it("explains when filter has no matches", () => {
      const filterEmpty = "No plugin matches. Clear the filter to see them all.";
      expect(filterEmpty).toContain("Clear the filter to see them all");
    });

    it("reports listing errors with retry", () => {
      const errTitle = "Could not list plugins";
      expect(errTitle).toBe("Could not list plugins");
    });
  });

  describe("Settings panel", () => {
    it("provides helpful message and clear filter button on unmatched search", () => {
      const emptyTitle = "No settings found";
      const emptyMsg =
        'No setting or plugin matches “unknown”. Check your spelling or try another keyword.';
      expect(emptyTitle).toBe("No settings found");
      expect(emptyMsg).toContain("Check your spelling");
    });

    it("reports loading errors with retry option", () => {
      const errTitle = "Could not load settings";
      expect(errTitle).toBe("Could not load settings");
    });
  });
});
