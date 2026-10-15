import { Type, type Static } from "typebox";

const text = Type.String({ minLength: 1 });
const positive = Type.Integer({ minimum: 1, maximum: 2147483647 });
const toggle = Type.Object({ enabled: Type.Boolean() }, { additionalProperties: false });
const rates = { input: Type.Number(), output: Type.Number(), cacheRead: Type.Number(), cacheWrite: Type.Number() };
const cost = Type.Object({
  ...rates, tiers: Type.Optional(Type.Array(Type.Object({
    ...rates, inputTokensAbove: Type.Number({ minimum: 0 }),
  }, { additionalProperties: false }))),
}, { additionalProperties: false });
export const ProviderSchema = Type.Object({
  id: text, baseUrl: Type.Optional(text), api: Type.Optional(text), authHeader: Type.Optional(Type.Boolean()),
  models: Type.Array(Type.Object({
    id: text, name: text, reasoning: Type.Boolean(),
    input: Type.Array(Type.Union([Type.Literal("text"), Type.Literal("image")])),
    cost, contextWindow: positive, maxTokens: positive,
  }, { additionalProperties: false }), { minItems: 1 }),
}, { additionalProperties: false });
export const ConfigSchema = Type.Object({
  network: Type.Optional(Type.Object({
    proxy: Type.Optional(Type.Union([text, Type.Null()])),
    idleTimeoutMs: Type.Optional(Type.Integer({ minimum: 0, maximum: 2147483647 })),
    probeTimeoutMs: Type.Optional(positive),
  }, { additionalProperties: false })),
  shellPath: Type.Optional(Type.Union([Type.String(), Type.Null()])),
  oauthTimeoutMs: positive,
  usageDir: Type.String(),
  permissions: Type.Object({ defaultMode: Type.Union([
    Type.Literal("deny"), Type.Literal("manual"), Type.Literal("auto-current"), Type.Literal("auto-all"),
  ]), timeoutMs: positive, summaryChars: positive }, { additionalProperties: false }),
  sandbox: Type.Object({ probeTimeoutMs: positive, probeCacheMs: positive, timeoutMs: positive, maxOutputBytes: Type.Integer({ minimum: 1, maximum: 1048576 }),
    shell: Type.Union([Type.Literal("sh"), Type.Literal("bash")]) }, { additionalProperties: false }),
  subagents: Type.Object({ maxConcurrent: positive, maxDepth: Type.Integer({ minimum: 0 }),
    summaryChars: positive }, { additionalProperties: false }),
  skills: Type.Object({ paths: Type.Array(text) }, { additionalProperties: false }),
  experts: Type.Object({ enabled: Type.Boolean(), prompt: Type.String() }, { additionalProperties: false }),
  web: Type.Object({ timeoutMs: positive, maxBytes: positive, maxRedirects: Type.Integer({ minimum: 0 }),
    search: Type.Object({ endpoint: text, credential: text, maxResults: positive }, { additionalProperties: false }),
  }, { additionalProperties: false }),
  sessionDir: text,
  maxSessions: positive,
  maxQueuedMessages: Type.Integer({ minimum: 0 }),
  idleTimeoutMs: positive,
  turnIdleTimeoutMs: positive,
  turnMaxMs: Type.Integer({ minimum: 0, maximum: 2147483647 }),
  maxLineBytes: positive,
  tools: Type.Array(text, { uniqueItems: true }),
  defaultToolProfile: text,
  toolProfiles: Type.Record(text, Type.Array(text, { uniqueItems: true })),
  attachments: Type.Object({ maxCount: Type.Integer({ minimum: 0 }), maxBytes: positive,
    maxTotalBytes: positive, mimeTypes: Type.Array(Type.Union([
      Type.Literal("image/png"), Type.Literal("image/jpeg"), Type.Literal("image/gif"), Type.Literal("image/webp"),
    ]), { uniqueItems: true }) }, { additionalProperties: false }),
  defaultModel: Type.Object({ provider: text, id: text }, { additionalProperties: false }),
  thinkingLevel: Type.Union([Type.Literal("off"), Type.Literal("minimal"), Type.Literal("low"), Type.Literal("medium"), Type.Literal("high"), Type.Literal("xhigh")]),
  logLevel: Type.Union([Type.Literal("debug"), Type.Literal("info"), Type.Literal("warn"), Type.Literal("error"), Type.Literal("silent")]),
  extensions: toggle,
  providers: Type.Array(ProviderSchema),
  retry: Type.Object({
    enabled: Type.Boolean(),
    maxRetries: Type.Optional(Type.Integer({ minimum: 0 })),
    baseDelayMs: Type.Optional(positive),
    maxAgentDelayMs: Type.Optional(positive),
  }, { additionalProperties: false }),
  compaction: toggle,
}, { additionalProperties: false });
export const ConfigPatchSchema = Type.Partial(ConfigSchema, { additionalProperties: false });
export const CredentialsSchema = Type.Record(Type.String({ minLength: 1 }), Type.Union([text, Type.Null()]));
export type Config = Static<typeof ConfigSchema>;
export type ConfigPatch = Static<typeof ConfigPatchSchema>;
export type Credentials = Static<typeof CredentialsSchema>;
