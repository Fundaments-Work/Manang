import { readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";

function svelteFiles(dir: string): string[] {
  const out: string[] = [];
  for (const name of readdirSync(dir)) {
    const path = join(dir, name);
    if (statSync(path).isDirectory()) out.push(...svelteFiles(path));
    else if (name.endsWith(".svelte")) out.push(path);
  }
  return out;
}

describe("WAI-ARIA & Accessibility audit", () => {
  const files = svelteFiles("src");

  it("ensures every dialog element specifies aria-modal and an accessible label", () => {
    const dialogRegex = /<([a-z0-9-]+)\b([^>]*\brole=["'](?:dialog|alertdialog)["'][^>]*)>/gi;
    const violations: string[] = [];

    for (const file of files) {
      const content = readFileSync(file, "utf8");
      let match;
      while ((match = dialogRegex.exec(content)) !== null) {
        const attrs = match[2] ?? "";
        const hasAriaModal = /aria-modal\s*=\s*["']true["']/i.test(attrs);
        const hasLabel = /aria-labelledby\s*=/i.test(attrs) || /aria-label\s*=/i.test(attrs);

        if (!hasAriaModal || !hasLabel) {
          const line = content.slice(0, match.index).split("\n").length;
          violations.push(`${file}:${line} -> <${match[1]} ${attrs}>`);
        }
      }
    }

    expect(violations).toEqual([]);
  });

  it("ensures icon-only buttons have an accessible name (aria-label or aria-labelledby)", () => {
    const buttonRegex = /<button\b([^>]*)>([\s\S]*?)<\/button>/g;
    const unlabeled: string[] = [];

    for (const file of files) {
      const content = readFileSync(file, "utf8");
      let match;
      while ((match = buttonRegex.exec(content)) !== null) {
        const attrs = match[1] ?? "";
        const body = (match[2] ?? "").trim();

        const hasAriaLabel = /aria-label\s*=/i.test(attrs) || /aria-labelledby\s*=/i.test(attrs);

        const strippedBody = body
          .replace(/<!--[\s\S]*?--!?>/g, "")
          .replace(/<svg[\s\S]*?<\/svg>/gi, "")
          .replace(/<[A-Z][a-zA-Z0-9]*Icon\b[^>]*\/>/g, "")
          .replace(/<[A-Z][a-zA-Z0-9]*Icon\b[^>]*>[\s\S]*?<\/[A-Z][a-zA-Z0-9]*Icon>/g, "")
          .replace(/<span\s+class="[^"]*icon[^"]*"[\s\S]*?<\/span>/gi, "")
          .replace(/<span\b[^>]*><\/span>/g, "")
          .replace(/\{#if[\s\S]*?\{\/if\}/g, "")
          .trim();

        // If strippedBody has no text left, it is an icon-only button!
        const isIconOnly = strippedBody.length === 0;

        if (isIconOnly && !hasAriaLabel) {
          const line = content.slice(0, match.index).split("\n").length;
          unlabeled.push(`${file}:${line}`);
        }
      }
    }

    expect(unlabeled).toEqual([]);
  });

  it("ensures live notices in StatusBar provide aria-live and status/alert role", () => {
    const statusBarContent = readFileSync("src/lib/components/StatusBar.svelte", "utf8");
    expect(statusBarContent).toContain("aria-live");
    expect(statusBarContent).toContain("role={notice.kind === \"error\" ? \"alert\" : \"status\"}");
  });

  it("ensures SearchPanel inline option toggles have explicit aria-labels", () => {
    const searchPanelContent = readFileSync("src/lib/components/panels/SearchPanel.svelte", "utf8");
    expect(searchPanelContent).toContain('aria-label="Match case"');
    expect(searchPanelContent).toContain('aria-label="Match whole word"');
    expect(searchPanelContent).toContain('aria-label="Use regular expression"');
  });

  it("ensures MermaidDialog includes focus trap and restore focus", () => {
    const mermaidDialogContent = readFileSync("src/lib/components/figure/MermaidDialog.svelte", "utf8");
    expect(mermaidDialogContent).toContain('role="dialog"');
    expect(mermaidDialogContent).toContain('aria-modal="true"');
    expect(mermaidDialogContent).toContain("returnTo");
    expect(mermaidDialogContent).toContain('event.key === "Tab"');
  });
});
