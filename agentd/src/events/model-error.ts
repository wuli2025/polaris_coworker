import { ModelsError, type AssistantMessage } from "@earendil-works/pi-ai";

const messages = {
  MODEL_AUTH: "模型身份验证失败，请检查提供商凭据与访问权限。",
  MODEL_RATE_LIMIT: "模型请求受到限流，请稍后重试。",
  MODEL_CONTEXT_OVERFLOW: "模型上下文超出限制，请压缩对话或开启新会话。",
  MODEL_NETWORK: "模型网络连接失败，请检查网络后重试。",
  MODEL_ERROR: "模型请求失败，请稍后重试。",
};
type Code = keyof typeof messages;
const categories: [Code, readonly string[]][] = [
  ["MODEL_AUTH", ["authentication_error", "authorization_error", "invalid_api_key", "permission_denied",
    "AuthenticationError", "PermissionDeniedError", "AccessDeniedException", "UnrecognizedClientException"]],
  ["MODEL_RATE_LIMIT", ["rate_limit_error", "rate_limit_exceeded", "RateLimitError", "ThrottlingException", "TooManyRequestsException"]],
  ["MODEL_CONTEXT_OVERFLOW", ["context_length_exceeded", "context_window_exceeded", "context_overflow", "ContextWindowExceededError"]],
  ["MODEL_NETWORK", ["provider_transport_failure", "APIConnectionError", "APIConnectionTimeoutError", "TimeoutError",
    "ECONNRESET", "ECONNREFUSED", "ENOTFOUND", "EAI_AGAIN", "ETIMEDOUT", "UND_ERR_CONNECT_TIMEOUT"]],
];

// Registered credential values are additionally removed by the daemon event sink.
function reason(message: AssistantMessage): string {
  const raw = message.errorMessage?.trim();
  if (!raw) return "";
  return raw.replace(/(?:authorization|x-api-key|api[_-]?key|access[_-]?token|refresh[_-]?token)\s*[=:]\s*[^\r\n,;}]+/gi, "[REDACTED]")
    .replace(/Bearer\s+[^\s,;]+/gi, "Bearer [REDACTED]")
    .replace(/sk-[a-zA-Z0-9_-]+/g, "[REDACTED]")
    .replace(/[\r\n\x00-\x1f]+/g, " ");
}
export function modelError(message: AssistantMessage): { code: Code; message: string } {
  const detail = reason(message);
  const describe = (code: Code) => ({ code, message: messages[code] + (detail ? ` 提供商原因：${detail}` : "") });
  // Pi 1.0.4 exposes structured failures through diagnostics. Some adapters only
  // supply errorMessage; leave those unclassified instead of parsing provider prose.
  for (const diagnostic of [...(message.diagnostics ?? [])].reverse()) {
    const details = diagnostic.details;
    const nested = details?.error;
    const structured = nested && typeof nested === "object" && !Array.isArray(nested) ? nested : undefined;
    const tags = [diagnostic.error?.code, diagnostic.error?.name, details?.errorType, details?.errorCode,
      details?.code, details?.type, structured && "code" in structured ? structured.code : undefined,
      structured && "type" in structured ? structured.type : undefined, diagnostic.type];
    const status = details?.status ?? details?.statusCode ?? details?.httpStatusCode;
    let code: Code | undefined;
    if (status === 401 || status === 403) code = "MODEL_AUTH";
    else if (status === 429) code = "MODEL_RATE_LIMIT";
    else code = categories.find(([, names]) => tags.some((tag) => typeof tag === "string" && names.includes(tag)))?.[0];
    if (code) return describe(code);
  }
  return describe("MODEL_ERROR");
}

/** SDK auth failures can happen before an assistant message exists. */
export function modelException(error: unknown): { code: string; message: string } | undefined {
  if (error instanceof ModelsError && ["auth", "oauth"].includes(error.code)) {
    return { code: "MODEL_AUTH", message: messages.MODEL_AUTH };
  }
  return undefined;
}
