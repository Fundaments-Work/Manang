/**
 * Wire types of the `agent.*` commands and events (`crates/manang-agent`).
 * Argument and field names are the Rust side's, snake_case included.
 */

export interface WireDiagnostic {
  readonly path: string;
  readonly line: number;
  readonly column?: number;
  readonly severity: string;
  readonly message: string;
  /** The lines around the problem. */
  readonly context?: string;
}

/** Something the author added with `@`. */
export type Mention =
  | { readonly kind: "file"; readonly path: string; readonly text?: string }
  | { readonly kind: "problems"; readonly diagnostics: readonly WireDiagnostic[] }
  | { readonly kind: "figure"; readonly path?: string };

/** What the UI sends with every message; the host adds the session header once. */
export interface TurnContext {
  /** Figure tools are offered only in the figure studio. */
  readonly view?: "document" | "figure";
  readonly active?: { readonly path: string; readonly line?: number };
  readonly selection?: {
    readonly path: string;
    readonly text: string;
    readonly line_from?: number;
    readonly line_to?: number;
  };
  readonly mentions?: readonly Mention[];
  /** Open documents with unsaved changes: path to text. The tools read these, not the disk. */
  readonly buffers?: Readonly<Record<string, string>>;
  /** The editor's problems (what the `diagnostics` tool reports). */
  readonly diagnostics?: readonly WireDiagnostic[];
}

export interface StartOptions {
  readonly model?: string;
  readonly max_iterations?: number;
  readonly max_tokens?: number;
}

export interface StartArgs {
  readonly session_id?: string;
  /** What the author typed (after the command name, when there is one). */
  readonly prompt?: string;
  /** A slash command (`rewrite`): its template becomes the first message. */
  readonly command?: string;
  readonly context: TurnContext;
  readonly options?: StartOptions;
  readonly project_root?: string;
  /** Run the conversation again as it stands (after a connection error). */
  readonly retry?: boolean;
}

export interface SlashCommand {
  readonly name: string;
  readonly description: string;
  /** What to type after the name (`<language>`). */
  readonly hint: string;
  readonly source: "builtin" | "user" | "project";
}

export interface KeyStatus {
  readonly present: boolean;
  readonly store: "keychain" | "kernel" | "session";
  readonly saved: boolean;
  readonly notice?: string;
}

export interface PresetInfo {
  readonly id: string;
  readonly name: string;
  readonly models: readonly string[];
  readonly base_url: string | null;
  readonly key_required: boolean;
  readonly local: boolean;
  readonly notes: string;
}

export interface AgentStatus {
  readonly enabled: boolean;
  readonly ready: boolean;
  /** `disabled`, `key_missing`, `consent_required`, `offline_blocked`, `model_missing`, `policy`. */
  readonly problems: readonly string[];
  readonly provider: string;
  readonly provider_name: string;
  readonly model: string | null;
  readonly base_url: string;
  readonly engine: string;
  readonly offline: boolean;
  readonly local: boolean;
  readonly key_required: boolean;
  readonly key: KeyStatus | null;
  readonly consent: boolean;
  readonly consent_text: string;
  /** The working window in tokens; compaction thresholds are shares of it. */
  readonly context_window: number;
  readonly include_thinking: boolean;
  readonly presets: readonly PresetInfo[];
  /** Present only in the browser build, where the provider is a labelled mock. */
  readonly mock?: boolean;
}

export interface WireRedaction {
  readonly kind: string;
  readonly start: number;
  readonly end: number;
  readonly replacement: string;
}

/** One part of what is sent: the selection, a mention. */
export interface PreviewPart {
  readonly id: string;
  readonly kind: string;
  readonly label: string;
  readonly tokens: number;
  readonly truncated: boolean;
  readonly text: string;
  readonly original?: string;
  readonly redactions: readonly WireRedaction[];
}

export interface RequestPreview {
  readonly provider: string;
  readonly model: string;
  readonly url: string;
  readonly headers: readonly (readonly [string, string])[];
  readonly body: string;
  readonly system: string | null;
  readonly tools: readonly { readonly name: string; readonly risk: string }[];
  readonly parts: readonly PreviewPart[];
  readonly redaction_count: number;
  readonly tokens: { readonly context: number; readonly request: number };
  readonly cost: {
    readonly est_input_micro_usd: number | null;
    readonly estimate: true;
    readonly note: string;
  };
  readonly mock?: boolean;
}

/** One located change of a proposal (offsets are UTF-16, the host's, in the text it read). */
export interface EditHunk {
  readonly from: number;
  readonly to: number;
  readonly before: string;
  readonly after: string;
}

/** A proposed text edit: one `edit_text` call, one or more hunks in one file. */
export interface TextEdit {
  readonly kind?: "text_edit";
  readonly path: string;
  readonly note: string;
  readonly edits: readonly EditHunk[];
}

export interface ConnectionResult {
  readonly ok: boolean;
  readonly code?: string;
  readonly message?: string;
  readonly model?: string;
  readonly latency_ms?: number;
  readonly request_id?: string | null;
}

export interface SessionMeta {
  readonly id: string;
  readonly title: string;
  readonly provider: string;
  readonly model: string;
  readonly state: string;
  readonly updated_ms: number;
}
