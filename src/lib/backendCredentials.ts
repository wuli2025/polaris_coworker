// Docker/Web 数据面共用的凭据来源。机器口令优先；没有机器口令时，统一账号登录得到的
// owner session 也能访问 /api/invoke、/ws、上传与文件，不再出现“明明已登录仍 401”。
export const MACHINE_TOKEN_KEY = "POLARIS_AUTH_TOKEN";
export const COLLAB_SESSION_TOKEN_KEY = "polaris.collab.token.v1";
export const BACKEND_CREDENTIAL_EVENT = "polaris:backend-credential-changed";

export function readBackendToken(): string | null {
  if (typeof window === "undefined") return null;
  try {
    const machine = localStorage.getItem(MACHINE_TOKEN_KEY)?.trim();
    if (machine) return machine;
    const account = localStorage.getItem(COLLAB_SESSION_TOKEN_KEY)?.trim();
    return account || null;
  } catch {
    return null;
  }
}

export function writeMachineToken(token: string): void {
  localStorage.setItem(MACHINE_TOKEN_KEY, token.trim());
  notifyBackendCredentialChanged();
}

export function notifyBackendCredentialChanged(): void {
  if (typeof window !== "undefined") {
    window.dispatchEvent(new Event(BACKEND_CREDENTIAL_EVENT));
  }
}
