/**
 * In-memory `settings`, `keybindings`, and `theme` plugin.
 *
 * Reproduces `crates/manang-settings` command surface (14 commands)
 * over `MockProjectHost` in browser and tests.
 */

import { DEFAULT_BINDINGS } from "$lib/commands/defaults";

import type { MockPlugin, MockPluginFactory } from "./mock";
import type { MockProjectHost } from "./mockProject";
import type {
  KeybindingConflict,
  KeybindingItem,
  KeybindingsListResponse,
  SettingDef,
  SettingDescription,
  SettingScope,
  SettingsGetAllResponse,
  SettingsSchema,
  ThemeDetail,
  ThemeListResponse,
} from "./api";

const COMMANDS: readonly (readonly [string, string])[] = [
  ["settings.get", "Get effective settings value or all values"],
  ["settings.set", "Set a user or project setting value"],
  ["settings.reset", "Reset a setting to its default"],
  ["settings.schema", "Get full settings schema"],
  ["settings.register", "Register plugin settings"],
  ["settings.open-file", "Open user, project or keybindings file"],
  ["settings.use-project", "Set active project for project-scoped settings"],
  ["keybindings.list", "List all keybindings and conflicts"],
  ["keybindings.resolve", "Resolve key press in active context"],
  ["keybindings.set", "Set keybinding override"],
  ["keybindings.reset", "Reset keybinding override"],
  ["keybindings.register", "Register default keybindings"],
  ["theme.list", "List available themes"],
  ["theme.get", "Get full theme token definition"],
];

const CORE_DEFINITIONS: SettingDef[] = [
  {
    id: "appearance.theme",
    type: "enum",
    values: ["dark", "light", "system"],
    default: "dark",
    scope: "user",
    "requires-restart": false,
    owner: "core",
    category: "appearance",
    description: "Colour theme. `system` follows the operating system.",
  },
  {
    id: "appearance.custom-theme",
    type: "string",
    default: "",
    scope: "user",
    "requires-restart": false,
    owner: "core",
    category: "appearance",
    description: "Id of a user theme from the themes folder to use instead of the built-in one. Empty for none.",
  },
  {
    id: "appearance.ui-scale",
    type: "float",
    min: 0.75,
    max: 2.0,
    default: 1.0,
    scope: "user",
    "requires-restart": false,
    owner: "core",
    category: "appearance",
    description: "Scale factor for the whole interface.",
  },
  {
    id: "appearance.reduced-motion",
    type: "enum",
    values: ["system", "on", "off"],
    default: "system",
    scope: "user",
    "requires-restart": false,
    owner: "core",
    category: "appearance",
    description: "Replace transitions with instant changes. `system` follows the operating system.",
  },
  {
    id: "editor.font-size",
    type: "int",
    min: 8,
    max: 48,
    default: 14,
    scope: "project",
    "requires-restart": false,
    owner: "core",
    category: "editor",
    description: "Editor font size in pixels.",
  },
  {
    id: "editor.line-height",
    type: "float",
    min: 1.0,
    max: 3.0,
    default: 1.6,
    scope: "project",
    "requires-restart": false,
    owner: "core",
    category: "editor",
    description: "Editor line height as a multiple of the font size.",
  },
  {
    id: "editor.tab-size",
    type: "int",
    min: 1,
    max: 16,
    default: 2,
    scope: "project",
    "requires-restart": false,
    owner: "core",
    category: "editor",
    description: "Number of columns a tab occupies.",
  },
  {
    id: "editor.word-wrap",
    type: "bool",
    default: true,
    scope: "project",
    "requires-restart": false,
    owner: "core",
    category: "editor",
    description: "Wrap long lines at the editor width.",
  },
  {
    id: "editor.line-numbers",
    type: "bool",
    default: true,
    scope: "project",
    "requires-restart": false,
    owner: "core",
    category: "editor",
    description: "Show line numbers in the gutter.",
  },
  {
    id: "editor.vim-mode",
    type: "bool",
    default: false,
    scope: "user",
    "requires-restart": false,
    owner: "core",
    category: "editor",
    description: "Use Vim-style modal editing. Off keeps the editor modeless.",
  },
  {
    id: "preview.auto-compile",
    type: "bool",
    default: true,
    scope: "project",
    "requires-restart": false,
    owner: "core",
    category: "preview",
    description: "Recompile the preview automatically while typing.",
  },
  {
    id: "preview.debounce-ms",
    type: "int",
    min: 0,
    max: 5000,
    default: 300,
    scope: "project",
    "requires-restart": false,
    owner: "core",
    category: "preview",
    description: "Milliseconds to wait after the last keystroke before recompiling.",
  },
  {
    id: "compile.engine",
    type: "enum",
    values: ["typst", "latex", "markdown"],
    default: "typst",
    scope: "project",
    "requires-restart": false,
    owner: "core",
    category: "compile",
    description: "Default compile engine for new projects.",
  },
  {
    id: "files.autosave",
    type: "enum",
    values: ["off", "after-delay", "on-focus-loss"],
    default: "after-delay",
    scope: "user",
    "requires-restart": false,
    owner: "core",
    category: "files",
    description: "When modified documents are saved automatically.",
  },
  {
    id: "files.trash-on-delete",
    type: "bool",
    default: true,
    scope: "user",
    "requires-restart": false,
    owner: "core",
    category: "files",
    description: "Move deleted files to the trash instead of removing them permanently.",
  },
];

/** The shipped defaults, shared with the frontend so the two cannot differ. */
const DEFAULT_KEYBINDINGS: KeybindingItem[] = DEFAULT_BINDINGS.map((d) => ({
  command: d.command,
  key: d.key,
  display: d.key.replace("Mod+", "Ctrl+"),
  when: "",
  source: "default",
}));

const THEME_DARK: ThemeDetail = {
  id: "dark",
  name: "Dark",
  scheme: "dark",
  builtin: true,
  base: null,
  tokens: {
    "--mng-bg": "#141416",
    "--mng-surface": "#1c1c20",
    "--mng-surface-hi": "#28282c",
    "--mng-surface-muted": "#353539",
    "--mng-recessed": "#0d0f0a",
    "--mng-line": "#3d3d42",
    "--mng-line-subtle": "#323236",
    "--mng-text": "#f5f4ef",
    "--mng-text-60": "#a6a5a0",
    "--mng-text-muted": "#757470",
    "--mng-accent": "#c6f135",
    "--mng-accent-ink": "#141416",
    "--mng-math": "#b3d17a",
  },
  overrides: {},
};

const THEME_LIGHT: ThemeDetail = {
  id: "light",
  name: "Light",
  scheme: "light",
  builtin: true,
  base: null,
  tokens: {
    "--mng-bg": "#faf9f7",
    "--mng-surface": "#f2f1ed",
    "--mng-surface-hi": "#e8e7e2",
    "--mng-surface-muted": "#dedcd6",
    "--mng-recessed": "#ecebe6",
    "--mng-line": "#d6d4cd",
    "--mng-line-subtle": "#e2e0d8",
    "--mng-text": "#1a1a1c",
    "--mng-text-60": "#5a595d",
    "--mng-text-muted": "#828087",
    "--mng-accent": "#586725",
    "--mng-accent-ink": "#faf9f7",
    "--mng-math": "#586725",
  },
  overrides: {},
};

function validateValue(def: SettingDef, value: unknown): unknown {
  switch (def.type) {
    case "bool":
      if (typeof value !== "boolean") {
        throw { message: `expected boolean for \`${def.id}\`` };
      }
      return value;
    case "int": {
      if (typeof value !== "number" || !Number.isInteger(value)) {
        throw { message: `expected integer for \`${def.id}\`` };
      }
      if (def.min !== undefined && value < def.min) {
        throw { message: `value ${value} is below minimum ${def.min} for \`${def.id}\`` };
      }
      if (def.max !== undefined && value > def.max) {
        throw { message: `value ${value} is above maximum ${def.max} for \`${def.id}\`` };
      }
      return value;
    }
    case "float": {
      if (typeof value !== "number" || !Number.isFinite(value)) {
        throw { message: `expected number for \`${def.id}\`` };
      }
      if (def.min !== undefined && value < def.min) {
        throw { message: `value ${value} is below minimum ${def.min} for \`${def.id}\`` };
      }
      if (def.max !== undefined && value > def.max) {
        throw { message: `value ${value} is above maximum ${def.max} for \`${def.id}\`` };
      }
      return value;
    }
    case "enum": {
      if (typeof value !== "string" || !(def.values ?? []).includes(value)) {
        throw { message: `value ${JSON.stringify(value)} is not valid for \`${def.id}\` (allowed: ${(def.values ?? []).join(", ")})` };
      }
      return value;
    }
    case "string":
    case "path": {
      if (typeof value !== "string") {
        throw { message: `expected string for \`${def.id}\`` };
      }
      return value;
    }
    case "list": {
      if (!Array.isArray(value)) {
        throw { message: `expected array for \`${def.id}\`` };
      }
      return value;
    }
    default:
      return value;
  }
}

export function settingsPlugin(host: MockProjectHost): MockPluginFactory {
  const definitions = new Map<string, SettingDef>(
    CORE_DEFINITIONS.map((d) => [d.id, d]),
  );

  const userValues = new Map<string, unknown>();
  const projectValues = new Map<string, unknown>();
  let activeProjectPath: string | null = null;

  const userKeybindings = new Map<string, { key: string; when: string }>();

  function resolveSetting(id: string): SettingDescription | null {
    const def = definitions.get(id);
    if (!def) return null;

    if (activeProjectPath && projectValues.has(id)) {
      return {
        id,
        value: projectValues.get(id),
        source: "project",
        default: def.default,
        "requires-restart": def["requires-restart"],
      };
    }
    if (userValues.has(id)) {
      return {
        id,
        value: userValues.get(id),
        source: "user",
        default: def.default,
        "requires-restart": def["requires-restart"],
      };
    }
    return {
      id,
      value: def.default,
      source: "default",
      default: def.default,
      "requires-restart": def["requires-restart"],
    };
  }

  return (emit): MockPlugin => ({
    name: "settings",
    commands: COMMANDS,
    handlers: {
      "settings.schema": () => {
        const categories = [...new Set([...definitions.values()].map((d) => d.category))];
        const response: SettingsSchema = {
          version: 1,
          categories,
          settings: [...definitions.values()],
        };
        return response;
      },

      "settings.get": (args) => {
        const id = typeof args.id === "string" ? args.id : undefined;
        if (id !== undefined) {
          const res = resolveSetting(id);
          if (!res) {
            throw { message: `unknown setting \`${id}\`` };
          }
          return res;
        }

        const values: Record<string, { value: unknown; source: "default" | "user" | "project" }> = {};
        for (const [key] of definitions) {
          const desc = resolveSetting(key);
          if (desc) {
            values[key] = { value: desc.value, source: desc.source };
          }
        }
        const response: SettingsGetAllResponse = {
          values,
          warnings: [],
        };
        return response;
      },

      "settings.set": (args) => {
        const id = String(args.id);
        const def = definitions.get(id);
        if (!def) {
          throw { message: `unknown setting \`${id}\`` };
        }
        const scope = (args.scope as SettingScope) || "user";
        if (scope === "project" && !activeProjectPath && !host.current) {
          throw { message: "cannot set project setting without an open project" };
        }
        if (scope === "project" && def.scope !== "project") {
          throw { message: `setting \`${id}\` is not project-scoped` };
        }

        const validated = validateValue(def, args.value);
        if (scope === "project") {
          projectValues.set(id, validated);
        } else {
          userValues.set(id, validated);
        }

        const desc = resolveSetting(id)!;
        emit("settings.changed", {
          id,
          value: desc.value,
          scope: desc.source,
          "requires-restart": def["requires-restart"],
        });

        return {
          ...desc,
          written: scope,
        };
      },

      "settings.reset": (args) => {
        const id = String(args.id);
        const def = definitions.get(id);
        if (!def) {
          throw { message: `unknown setting \`${id}\`` };
        }
        const scope = (args.scope as SettingScope) || "user";
        if (scope === "project") {
          projectValues.delete(id);
        } else {
          userValues.delete(id);
        }

        const desc = resolveSetting(id)!;
        emit("settings.changed", {
          id,
          value: desc.value,
          scope: desc.source,
          "requires-restart": def["requires-restart"],
        });
        return desc;
      },

      "settings.register": (args) => {
        const settings = Array.isArray(args.settings) ? (args.settings as SettingDef[]) : [];
        const owner = typeof args.owner === "string" ? args.owner : "plugin";
        let registered = 0;
        for (const s of settings) {
          if (!definitions.has(s.id)) {
            definitions.set(s.id, {
              ...s,
              owner,
              category: s.id.split(".")[0] ?? "other",
            });
            registered += 1;
          }
        }
        return { registered, warnings: [] };
      },

      "settings.open-file": (args) => {
        const scope = typeof args.scope === "string" ? args.scope : "user";
        const root = host.current?.root ?? "/home/user/project";
        let path = "/home/user/.config/manang/settings.toml";
        if (scope === "project") {
          path = `${root}/.manang/settings.toml`;
        } else if (scope === "keybindings") {
          path = "/home/user/.config/manang/keybindings.toml";
        }
        return { path, created: false };
      },

      "settings.use-project": (args) => {
        activeProjectPath = typeof args.path === "string" ? args.path : null;
        if (activeProjectPath) {
          try {
            host.open(activeProjectPath);
          } catch {
            // ignore
          }
        }
        return { warnings: [] };
      },

      "keybindings.list": () => {
        // As the registry does: a user entry replaces all of its command's
        // defaults; an entry with no keys unbinds.
        const bindings: KeybindingItem[] = DEFAULT_KEYBINDINGS.filter(
          (d) => !userKeybindings.has(d.command),
        );
        for (const [command, u] of userKeybindings) {
          if (u.key === "") continue;
          bindings.push({
            command,
            key: u.key,
            display: u.key.replace("Mod+", "Ctrl+"),
            when: u.when,
            source: "user",
          });
        }

        // Two commands on one key: a tie, or a user binding shadowing a default.
        const conflicts: KeybindingConflict[] = [];
        for (let i = 0; i < bindings.length; i++) {
          for (let j = i + 1; j < bindings.length; j++) {
            const x = bindings[i];
            const y = bindings[j];
            if (x === undefined || y === undefined) continue;
            if (x.command === y.command) continue;
            if (x.key.toLowerCase() !== y.key.toLowerCase()) continue;
            conflicts.push({
              key: x.display,
              kind: x.source === y.source ? "conflict" : "shadow",
              commands: [x.command, y.command].sort(),
            });
          }
        }
        const response: KeybindingsListResponse = {
          platform: "linux",
          bindings,
          conflicts,
          warnings: [],
        };
        return response;
      },

      "keybindings.resolve": (args) => {
        const key = typeof args.key === "string" ? args.key : "";
        for (const b of DEFAULT_KEYBINDINGS) {
          if (b.key === key || b.display === key) {
            return { command: b.command };
          }
        }
        return null;
      },

      "keybindings.set": (args) => {
        const command = String(args.command);
        const key = typeof args.key === "string" ? args.key : (Array.isArray(args.keys) ? String(args.keys[0] ?? "") : "");
        const when = typeof args.when === "string" ? args.when : "";

        userKeybindings.set(command, { key, when });
        emit("keybindings.changed", {});

        return {
          command,
          keys: [key],
          when,
          conflicts: [],
        };
      },

      "keybindings.reset": (args) => {
        const command = typeof args.command === "string" ? args.command : null;
        if (command) {
          userKeybindings.delete(command);
        } else {
          userKeybindings.clear();
        }
        emit("keybindings.changed", {});
        return { command };
      },

      "keybindings.register": (args) => {
        const bindings = Array.isArray(args.bindings) ? args.bindings : [];
        return { registered: bindings.length };
      },

      "theme.list": () => {
        const response: ThemeListResponse = {
          themes: [
            { id: THEME_DARK.id, name: THEME_DARK.name, scheme: THEME_DARK.scheme, builtin: true, base: null },
            { id: THEME_LIGHT.id, name: THEME_LIGHT.name, scheme: THEME_LIGHT.scheme, builtin: true, base: null },
          ],
          warnings: [],
        };
        return response;
      },

      "theme.get": (args) => {
        const id = typeof args.id === "string" ? args.id : "dark";
        if (id === "light") {
          return THEME_LIGHT;
        }
        return THEME_DARK;
      },
    },
  });
}
