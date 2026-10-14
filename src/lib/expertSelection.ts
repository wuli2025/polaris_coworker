import { expert, invoke } from "../tauri";

export type ExpertSelection = { kind: "expert" | "team"; id: string };

/** Resolve the backend before any legacy file-writing command. Failure never falls back. */
export async function applyExpertSelection(projectId: string, kind: ExpertSelection["kind"], id: string): Promise<void> {
  const status = await invoke<{ backend: string }>("agent_backend_status");
  if (status.backend === "pi") return;
  if (status.backend !== "claude-cli") throw new Error(`未知后端：${status.backend}`);
  if (kind === "team") await expert.teamApply(projectId, id, true);
  else await expert.apply(projectId, id, true);
}

export function selectedExpert(mode: string, expertId: string, teamId: string): ExpertSelection | undefined {
  if (mode === "single-expert" && expertId) return { kind: "expert", id: expertId };
  if (mode === "expert-team" && teamId) return { kind: "team", id: teamId };
  return undefined;
}

export function expertRequestFields(options: { expertSelection?: ExpertSelection }): { expertSelection?: ExpertSelection } {
  return { expertSelection: options.expertSelection };
}
