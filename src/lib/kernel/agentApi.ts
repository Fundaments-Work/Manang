/**
 * Typed wrappers over the `agent.*` commands (`crates/manang-agent/manifest.toml`).
 *
 * No behaviour: argument names and result shapes are the manifest's. A failed command rejects
 * with a `[code] message` the UI shows as plain text (`errorMessage`).
 */

import type {
  AgentStatus,
  ConnectionResult,
  RequestPreview,
  SessionMeta,
  SlashCommand,
  StartArgs,
  TextEdit,
} from "$lib/agent/types";

import type { Kernel } from "./client";

export function agentApi(kernel: Kernel) {
  return {
    /** Returns at once; the answer streams as `agent.*` events. */
    start: (args: StartArgs) =>
      kernel.run<{ session_id: string }>("agent.start", { ...args }),
    cancel: (sessionId: string) =>
      kernel.run<{ cancelled: boolean }>("agent.cancel", { session_id: sessionId }),
    approve: (callId: string, decision: "allow" | "allow_session" | "deny") =>
      kernel.run<{ ok: boolean }>("agent.approve", { call_id: callId, decision }),
    /** The edit to apply; the editor applies it as ONE transaction. */
    applyProposal: (proposalId: string) =>
      kernel.run<{ proposal_id: string; kind: string; edit: TextEdit }>(
        "agent.apply-proposal",
        { proposal_id: proposalId },
      ),
    rejectProposal: (proposalId: string) =>
      kernel.run<{ proposal_id: string; rejected: boolean }>("agent.reject-proposal", {
        proposal_id: proposalId,
      }),
    /** The exact outbound request, without sending it. */
    previewRequest: (args: StartArgs) =>
      kernel.run<RequestPreview>("agent.preview-request", { ...args }),
    /** The slash commands (built-ins, user files, project files). */
    commands: (projectRoot?: string) =>
      kernel.run<{ commands: SlashCommand[] }>("agent.commands", {
        ...(projectRoot === undefined ? {} : { project_root: projectRoot }),
      }),
    sessions: (projectRoot?: string) =>
      kernel.run<{ sessions: SessionMeta[] }>("agent.sessions", {
        ...(projectRoot === undefined ? {} : { project_root: projectRoot }),
      }),
    transcript: (sessionId: string, projectRoot?: string) =>
      kernel.run<{ session_id: string; messages: unknown[] }>("agent.transcript", {
        session_id: sessionId,
        ...(projectRoot === undefined ? {} : { project_root: projectRoot }),
      }),
    models: (provider?: string) =>
      kernel.run<{
        provider: string;
        models: string[];
        default: string | null;
        fetch_supported: boolean;
        note: string;
      }>("agent.models", provider === undefined ? {} : { provider }),
    testConnection: (projectRoot?: string) =>
      kernel.run<ConnectionResult>("agent.test-connection", {
        ...(projectRoot === undefined ? {} : { project_root: projectRoot }),
      }),
    status: (projectRoot?: string) =>
      kernel.run<AgentStatus>("agent.status", {
        ...(projectRoot === undefined ? {} : { project_root: projectRoot }),
      }),
    consent: (provider: string, granted = true) =>
      kernel.run<{ provider: string; consent: boolean }>("agent.consent", {
        provider,
        granted,
      }),
    /** Pushes setting values (the shell's settings sync does this in the desktop app). */
    configure: (values: Record<string, unknown>) =>
      kernel.run<{ applied: number }>("agent.configure", { values }),
    /** The key goes in and never comes back. */
    keySet: (provider: string, key: string) =>
      kernel.run<{ store: string; saved: boolean; notice?: string }>("agent.key.set", {
        provider,
        key,
      }),
    keyStatus: (provider: string) =>
      kernel.run<{ present: boolean; store: string; saved: boolean; notice?: string }>(
        "agent.key.status",
        { provider },
      ),
    keyClear: (provider: string) =>
      kernel.run<{ cleared: boolean }>("agent.key.clear", { provider }),
  };
}

export type AgentApi = ReturnType<typeof agentApi>;
