import { InMemoryCredentialStore, type CredentialStore } from "@earendil-works/pi-ai";
export type { CredentialStore };
export { InMemoryCredentialStore };

/** Hosts may inject their own store; P1 never creates auth.json. */
export function memoryCredentials(): CredentialStore {
  return new InMemoryCredentialStore();
}

export class Redactor {
  private readonly secrets = new Set<string>();
  add(secret: string): void {
    if (secret) this.secrets.add(secret);
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
