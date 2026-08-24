const TOKEN_SHAPE = /\bsk-[A-Za-z0-9_-]{16,}\b/g;

function isCredentialKey(key) {
  const normalized = String(key).replace(/[^A-Za-z0-9]/g, "").toLowerCase();
  if (/^(?:input|output|total|max|cached|cacheread|cachecreation)tokens?$/.test(normalized)) {
    return false;
  }
  return (
    normalized === "token" ||
    normalized.endsWith("token") ||
    /(?:authorization|password|secret|apikey)/.test(normalized)
  );
}

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
    if (isCredentialKey(key) && current != null) return "[REDACTED]";
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
