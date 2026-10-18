/**
 * Wire types of the `agent.*` commands and events (`crates/manang-agent`).
 * Argument and field names are the Rust side's, snake_case included.
 */

export type AgentFeature =
  | "chat"
  | "rewrite"
  | "shorten"
  | "translate"
  | "explain"
  | "fix-diagnostic";

export type ChipKind =
  | "selection"
  | "file"
  | "diagnostics"
  | "outline"
  | "bibliography-keys"
  | "figure"
  | "pinned";

export interface WireDiagnostic {
  readonly path: string;
  readonly line: number;
  readonly column?: number;
  readonly severity: string;
  readonly message: string;
  /** The lines around the problem. */
  readonly context?: string;
}

/** A context chip as sent to `agent.start`. */
export interface ChipInput {
  readonly id?: string;
  readonly kind: ChipKind;
  readonly label?: string;
  readonly path?: string;
  /** UTF-16 offsets (what CodeMirror counts). */
  readonly range?: readonly [number, number];
  readonly text?: string;
  readonly diagnostics?: readonly WireDiagnostic[];
  readonly pinned?: boolean;
}

/** A chip in the tray: what it is, its size, and whether it stays between requests. */
export interface TrayChip extends ChipInput {
  readonly id: string;
  readonly label: string;
  readonly tokens: number;
}

export interface StartOptions {
  readonly model?: string;
  readonly max_iterations?: number;
  readonly max_tokens?: number;
  readonly target_language?: string;
}

export interface StartArgs {
  readonly session_id?: string;
  readonly feature: AgentFeature;
  readonly prompt?: string;
  readonly context: readonly ChipInput[];
  readonly options?: StartOptions;
  readonly project_root?: string;
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
  readonly max_context_tokens: number;
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

export interface PreviewChip {
  readonly id: string;
  readonly kind: ChipKind;
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
  readonly chips: readonly PreviewChip[];
  readonly redaction_count: number;
  readonly tokens: { readonly context: number; readonly request: number };
  readonly cost: {
    readonly est_input_micro_usd: number | null;
    readonly estimate: true;
    readonly note: string;
  };
  readonly mock?: boolean;
}

/** A proposed text edit (offsets are UTF-16). */
export interface TextEdit {
  readonly kind?: "text_edit";
  readonly path: string;
  readonly from: number;
  readonly to: number;
  readonly before: string;
  readonly after: string;
  readonly explanation: string;
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
  readonly feature: string;
  readonly provider: string;
  readonly model: string;
  readonly state: string;
  readonly updated_ms: number;
}
