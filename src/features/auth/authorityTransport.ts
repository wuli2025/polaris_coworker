/**
 * Protect credentials sent directly from the browser to an account authority.
 * HTTPS and exact loopback HTTP are safe defaults; every other HTTP endpoint
 * requires the login UI's explicit one-time risk confirmation.
 */
export function validateAuthorityTransport(
  raw: string,
  allowInsecureHttp = false,
  baseOrigin = typeof window !== "undefined" ? window.location.origin : "https://localhost",
): string {
  let parsed: URL;
  try {
    parsed = new URL(raw, baseOrigin);
  } catch {
    throw new Error("账号中心地址无效");
  }
  if (parsed.protocol === "https:") return parsed.toString();

  const loopback =
    parsed.protocol === "http:" &&
    (parsed.hostname === "localhost" ||
      parsed.hostname === "127.0.0.1" ||
      parsed.hostname === "[::1]" ||
      parsed.hostname === "::1");
  if (loopback || (parsed.protocol === "http:" && allowInsecureHttp)) {
    return parsed.toString();
  }
  if (parsed.protocol === "http:") {
    throw new Error(
      `账号中心 ${parsed.origin} 使用明文 HTTP，邮箱和验证码可能被窃取。请改用 HTTPS；若明确接受旧服务风险，请在高级设置勾选后重试`,
    );
  }
  throw new Error("账号中心地址只支持 HTTPS（本机调试可用 HTTP loopback）");
}
