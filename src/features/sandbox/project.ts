import { sandbox } from "./api";

export function needsSandboxMigration(messages: string[]): boolean {
  return messages.some(message => /SANDBOX_(UNSAFE_MOUNT|UNSAFE_CONTAINER|PATH_UNMOUNTED|INVALID_MOUNTS)/.test(message));
}

export async function startProjectSandbox(projectDir: string, rebuild = false): Promise<string> {
  const directory = projectDir.trim();
  if (!directory) throw new Error("请选择或输入项目工作目录，再确认启动。");
  return sandbox.start(directory, rebuild);
}
