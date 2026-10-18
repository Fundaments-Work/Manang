<script lang="ts">
  import { onMount } from "svelte";

  import { parseKey } from "$lib/commands/keymap";
  import { formatShortcut } from "$lib/commands/shortcut";
  import ContextMenu, { type MenuItem } from "$lib/components/ui/ContextMenu.svelte";
  import Icon from "$lib/components/ui/Icon.svelte";
  import {
    onVisualActivity,
    runVisualCommand,
    visualHandle,
  } from "$lib/editor/visual/extension";
  import type { CommandState } from "$lib/editor/visual/protocol";
  import { dialog } from "$lib/state/dialog.svelte";
  import { documents } from "$lib/state/documents.svelte";
  import { workspace } from "$lib/state/workspace.svelte";
  import { detectPlatform } from "$lib/util/keyboard";
  import { extension } from "$lib/workbench/paths";
  import { figureSceneFor, visualFormat } from "$lib/workbench/visual";
  import {
    HEADING_LEVELS,
    headingArg,
    isDisabled,
    isPressed,
    itemsFor,
    relativeTo,
    tooltip,
    type ToolbarItem,
  } from "$lib/workbench/visualToolbar";

  /**
   * The Visual toolbar: one row under the tab strip, shown only while a document
   * is in Visual mode. Every button asks the visual session for an edit plan
   * (`visual.run-command`) and applies it as one transaction, so one click is
   * one undo step. Pressed state comes from `visual.command-state`.
   */
  interface Props {
    path: string;
  }
  const { path }: Props = $props();

  const platform = detectPlatform();
  const format = $derived(visualFormat(path) ?? "markdown");
  let states = $state.raw<readonly CommandState[]>([]);
  let level = $state(0);
  let status = $state<string>("starting");
  let menu = $state<{ x: number; y: number; label: string; items: MenuItem[] } | null>(
    null,
  );
  let timer: ReturnType<typeof setTimeout> | undefined;
  let alive = true;

  const items = $derived(itemsFor(format, states));

  function shortcutText(item: ToolbarItem): string | undefined {
    if (item.shortcut === undefined) return undefined;
    const parsed = parseKey(item.shortcut);
    return parsed === null ? undefined : formatShortcut(parsed, platform);
  }

  /** Re-read the command state a moment after the selection settles. */
  function schedule(): void {
    clearTimeout(timer);
    timer = setTimeout(() => void refresh(), 90);
  }

  async function refresh(): Promise<void> {
    const handle = visualHandle(documents.view);
    if (handle === null) {
      status = "starting";
      return;
    }
    status = handle.status;
    level = handle.headingLevel();
    const next = await handle.commandState();
    if (alive && handle === visualHandle(documents.view)) states = next;
  }

  onMount(() => {
    const off = onVisualActivity(schedule);
    schedule();
    return () => {
      alive = false;
      clearTimeout(timer);
      off();
    };
  });

  function run(command: string, args?: string): void {
    const view = documents.view;
    if (view !== undefined) void runVisualCommand(view, command, args);
  }

  function open(event: MouseEvent, label: string, list: MenuItem[]): void {
    const box = (event.currentTarget as HTMLElement).getBoundingClientRect();
    menu = { x: box.left, y: box.bottom, label, items: list };
  }

  function headingMenu(event: MouseEvent): void {
    const current = level;
    open(
      event,
      "Heading level",
      HEADING_LEVELS.map(({ level: n, label }) => ({
        label,
        checked: n === current,
        onselect: () => run("heading", headingArg(current, n)),
      })),
    );
  }

  function mathMenu(event: MouseEvent): void {
    open(event, "Formula", [
      { label: "Inline formula", onselect: () => run("math", "inline") },
      { label: "Display formula", onselect: () => run("math", "block") },
    ]);
  }

  async function linkPrompt(): Promise<void> {
    const answer = await dialog.prompt({
      title: "Link",
      message: "Where should the link go?",
      input: { label: "Address", value: "https://" },
      actions: [{ id: "add", label: "Add link", primary: true }],
    });
    const url = answer.value.trim();
    if (answer.id === "add" && url !== "") run("link", url);
  }

  async function otherPath(command: string): Promise<void> {
    const answer = await dialog.prompt({
      title: "Insert image",
      message: "Path of an image or an exported figure, relative to this file.",
      input: { label: "Path", value: "" },
      actions: [{ id: "add", label: "Insert", primary: true }],
    });
    const value = answer.value.trim();
    if (answer.id === "add" && value !== "") run(command, value);
  }

  /** Figures and images of the project, figures first. */
  function insertMenu(event: MouseEvent, command: string): void {
    const scenes = new Set(workspace.figures.map((f) => f.path));
    const images = workspace.paths.filter((p) =>
      ["svg", "png", "jpg", "jpeg", "gif", "webp"].includes(extension(p)),
    );
    const isFigure = (p: string): boolean => figureSceneFor(p, scenes) !== null;
    const sorted = [...images].sort(
      (a, b) => Number(isFigure(b)) - Number(isFigure(a)) || a.localeCompare(b),
    );
    const list: MenuItem[] = sorted.slice(0, 12).map((p) => ({
      label: isFigure(p) ? `Figure: ${p}` : p,
      onselect: () => run(command, relativeTo(path, p)),
    }));
    list.push({
      label: "Other path...",
      separated: list.length > 0,
      onselect: () => void otherPath(command),
    });
    open(event, "Insert image or figure", list);
  }

  function activate(item: ToolbarItem, event: MouseEvent): void {
    switch (item.control) {
      case "heading":
        headingMenu(event);
        return;
      case "math":
        mathMenu(event);
        return;
      case "link":
        void linkPrompt();
        return;
      case "insert":
        insertMenu(event, item.command);
        return;
      default:
        run(item.command, item.args);
    }
  }

  /** Roving focus (WAI-ARIA toolbar): one Tab stop, arrows move along the buttons. */
  let rove = $state(0);
  let bar: HTMLDivElement | undefined = $state();

  function onkeydown(event: KeyboardEvent): void {
    const keys = ["ArrowRight", "ArrowLeft", "Home", "End"];
    if (!keys.includes(event.key) || bar === undefined) return;
    const buttons = [
      ...bar.querySelectorAll<HTMLButtonElement>("button:not(:disabled)"),
    ];
    const at = buttons.indexOf(document.activeElement as HTMLButtonElement);
    if (buttons.length === 0 || at === -1) return;
    event.preventDefault();
    const next =
      event.key === "Home"
        ? 0
        : event.key === "End"
          ? buttons.length - 1
          : (at + (event.key === "ArrowRight" ? 1 : -1) + buttons.length) %
            buttons.length;
    buttons[next]?.focus();
  }

  const hasMenu = (item: ToolbarItem): boolean =>
    item.control === "heading" || item.control === "math" || item.control === "insert";
</script>

<div
  class="flex h-9 shrink-0 items-center gap-px border-b border-line bg-surface px-2"
  role="toolbar"
  aria-label="Formatting"
  data-testid="visual-toolbar"
  tabindex="-1"
  bind:this={bar}
  {onkeydown}
>
  {#each items as item, index (item.id)}
    {#if item.groupStart === true}
      <span class="mx-1 h-4 w-px bg-line" aria-hidden="true"></span>
    {/if}
    {@const text = tooltip(item.label, shortcutText(item))}
    <button
      type="button"
      class="mng-icon-btn inset-focus"
      data-visual={item.id}
      aria-label={item.label}
      aria-pressed={isPressed(states, item, level)}
      aria-haspopup={hasMenu(item) ? "menu" : undefined}
      aria-keyshortcuts={item.shortcut}
      title={text}
      tabindex={index === Math.min(rove, items.length - 1) ? 0 : -1}
      onfocus={() => (rove = index)}
      disabled={isDisabled(states, item.command)}
      onmousedown={(event) => event.preventDefault()}
      onclick={(event) => activate(item, event)}
    >
      <Icon name={item.icon} size={14} />
    </button>
  {/each}
  {#if status === "unavailable" || status === "degraded"}
    <span class="ml-auto pr-1 text-[11px] text-text-60" role="status">
      {status === "unavailable"
        ? "Visual is not available for this file"
        : "Visual paused, showing source"}
    </span>
  {/if}
</div>

{#if menu !== null}
  <ContextMenu
    x={menu.x}
    y={menu.y}
    label={menu.label}
    items={menu.items}
    onclose={() => (menu = null)}
  />
{/if}
