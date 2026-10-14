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
  turnTimeoutMs: positive,
  maxLineBytes: positive,
  tools: Type.Array(text, { uniqueItems: true }),
  defaultModel: Type.Object({ provider: text, id: text }, { additionalProperties: false }),
  thinkingLevel: Type.Union([Type.Literal("off"), Type.Literal("minimal"), Type.Literal("low"), Type.Literal("medium"), Type.Literal("high"), Type.Literal("xhigh")]),
  logLevel: Type.Union([Type.Literal("debug"), Type.Literal("info"), Type.Literal("warn"), Type.Literal("error"), Type.Literal("silent")]),
  extensions: toggle,
  providers: Type.Array(ProviderSchema),
  retry: toggle,
  compaction: toggle,
}, { additionalProperties: false });
export const ConfigPatchSchema = Type.Partial(ConfigSchema);
export const CredentialsSchema = Type.Record(Type.String({ minLength: 1 }), Type.Union([text, Type.Null()]));
export type Config = Static<typeof ConfigSchema>;
export type ConfigPatch = Static<typeof ConfigPatchSchema>;
export type Credentials = Static<typeof CredentialsSchema>;
