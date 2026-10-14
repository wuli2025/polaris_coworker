import { sandbox } from "./api";

export function needsSandboxMigration(messages: string[]): boolean {
  return messages.some(message => /SANDBOX_(UNSAFE_MOUNT|UNSAFE_CONTAINER|PATH_UNMOUNTED|INVALID_MOUNTS)/.test(message));
}

export async function startProjectSandbox(projectDir: string, rebuild = false): Promise<string> {
  const directory = projectDir.trim();
  if (!directory) throw new Error("请选择或输入项目工作目录，再确认启动。");
  return sandbox.start(directory, rebuild);
}

// Format guidance only: existence, host platform and mount safety belong to the backend.
export function projectDirectoryHint(value: string): string {
  const directory = value.trim();
  if (!directory) return "请选择或输入项目工作目录。";
  if (!directory.startsWith("/") && !/^[a-zA-Z]:[\\/]/.test(directory)) {
    return "请输入绝对路径，例如 /home/me/project 或 C:\\Projects\\app；目录是否可用以后端校验为准。";
  }
  return "";
}
