const CREDENTIAL_KEY = /(?:authorization|password|secret|token|api[_-]?key|auth[_-]?token)/i;
const TOKEN_SHAPE = /\bsk-[A-Za-z0-9_-]{16,}\b/g;

function redactString(value, secrets) {
  let output = value;
  for (const secret of secrets) {
    if (secret) output = output.split(secret).join("[REDACTED]");
  }
  return output.replace(TOKEN_SHAPE, "[REDACTED]");
}

export function redact(value, secrets = []) {
  const exact = [...new Set(secrets.filter((secret) => typeof secret === "string" && secret))]
    .sort((a, b) => b.length - a.length);

  function visit(current, key = "") {
    if (CREDENTIAL_KEY.test(key) && current != null) return "[REDACTED]";
    if (typeof current === "string") return redactString(current, exact);
    if (Array.isArray(current)) return current.map((item) => visit(item));
    if (current && typeof current === "object") {
      return Object.fromEntries(Object.entries(current).map(([name, item]) => [name, visit(item, name)]));
    }
    return current;
  }

  return visit(value);
}

export function redactText(value, secrets = []) {
  return redactString(String(value), secrets);
}
