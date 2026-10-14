import { Type, type Static } from "typebox";

const text = Type.String({ minLength: 1 });
const positive = Type.Integer({ minimum: 1, maximum: 2147483647 });
const toggle = Type.Object({ enabled: Type.Boolean() }, { additionalProperties: false });
const cost = Type.Object({
  input: Type.Number({ minimum: 0 }), output: Type.Number({ minimum: 0 }),
  cacheRead: Type.Number({ minimum: 0 }), cacheWrite: Type.Number({ minimum: 0 }),
}, { additionalProperties: false });
export const ProviderSchema = Type.Object({
  id: text, baseUrl: text, api: text,
  models: Type.Array(Type.Object({
    id: text, name: text, reasoning: Type.Boolean(),
    input: Type.Array(Type.Union([Type.Literal("text"), Type.Literal("image")])),
    cost, contextWindow: positive, maxTokens: positive,
  }, { additionalProperties: false }), { minItems: 1 }),
}, { additionalProperties: false });
export const ConfigSchema = Type.Object({
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
