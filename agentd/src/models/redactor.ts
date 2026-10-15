export class Redactor {
  private readonly secrets = new Set<string>();
  add(secret: string): void {
    if (!secret) return;
    // Register representations before any logger/event sink sees the credential.
    for (const variant of [secret, encodeURIComponent(secret),
      Buffer.from(secret, "utf8").toString("base64"), JSON.stringify(secret).slice(1, -1)]) {
      this.secrets.add(variant);
    }
  }
  text(value: string): string {
    for (const secret of [...this.secrets].sort((a, b) => b.length - a.length)) {
      value = value.split(secret).join("[REDACTED]");
    }
    return value;
  }
  clean<T>(value: T): T {
    if (typeof value === "string") return this.text(value) as T;
    if (Array.isArray(value)) return value.map((v) => this.clean(v)) as T;
    if (value && typeof value === "object") {
      return Object.fromEntries(Object.entries(value).map(([k, v]) => [this.text(k), this.clean(v)])) as T;
    }
    return value;
  }
}
