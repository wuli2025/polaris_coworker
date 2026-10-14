import type { ExtensionFactory } from "@earendil-works/pi-coding-agent";

/** Optional trusted host extensions. Mandatory permission/sandbox/web/subagent
 * factories are assembled per session in SessionRegistry and cannot be disabled
 * by extensions.enabled. Project executable extensions remain disabled. */
export const extensionFactories: ExtensionFactory[] = [];
