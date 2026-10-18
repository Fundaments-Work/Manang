/**
 * Templates workbench domain logic: filtering, searching and grouping.
 *
 * Keeps rules pure, data in, data out, so filtering and search are tested in
 * isolation without UI or IPC dependency.
 */

import type { Engine, TemplateSummary } from "$lib/kernel/types";

export type TemplateFilterEngine = Engine | "all";

export interface TemplateQuery {
  readonly query: string;
  readonly engine?: TemplateFilterEngine;
}

/** Case-insensitive query match against template name, description, tags, and engine. */
export function matchesTemplateQuery(
  template: TemplateSummary,
  query: string,
): boolean {
  const needle = query.trim().toLowerCase();
  if (needle === "") return true;
  return (
    template.name.toLowerCase().includes(needle) ||
    template.id.toLowerCase().includes(needle) ||
    template.description.toLowerCase().includes(needle) ||
    template.engine.toLowerCase().includes(needle) ||
    template.tags.some((tag) => tag.toLowerCase().includes(needle))
  );
}

/** Select and filter templates by text query and optional engine. */
export function selectTemplates(
  templates: readonly TemplateSummary[],
  { query, engine = "all" }: TemplateQuery,
): TemplateSummary[] {
  return templates.filter((t) => {
    if (engine !== "all" && t.engine !== engine) return false;
    return matchesTemplateQuery(t, query);
  });
}

/** Group templates by engine. */
export function groupTemplatesByEngine(
  templates: readonly TemplateSummary[],
): Record<Engine, TemplateSummary[]> {
  const groups: Record<Engine, TemplateSummary[]> = {
    typst: [],
    latex: [],
    markdown: [],
  };
  for (const t of templates) {
    groups[t.engine]?.push(t);
  }
  return groups;
}
