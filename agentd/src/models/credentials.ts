import { InMemoryCredentialStore, type CredentialStore } from "@earendil-works/pi-ai";
export type { CredentialStore };
export { InMemoryCredentialStore };

/** Hosts may inject their own store; P1 never creates auth.json. */
export function memoryCredentials(): CredentialStore {
  return new InMemoryCredentialStore();
}

export { Redactor } from "./redactor.ts";
