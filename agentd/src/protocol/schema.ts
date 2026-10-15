import { Type, type Static, type TSchema } from "typebox";
import { ConfigPatchSchema, CredentialsSchema } from "../config/schema.ts";
const text = Type.String({ minLength: 1 });
const id = Type.Union([text, Type.Integer()]);
const request = <M extends string, P extends Record<string, TSchema>>(method: M, params: P) =>
  Type.Object({ id, method: Type.Literal(method), params: Type.Object(params, { additionalProperties: false }) },
    { additionalProperties: false });
export const PROTOCOL_VERSION = 6;
export const ModelSchema = Type.Object({ provider: text, id: text }, { additionalProperties: false });
const nonnegative = Type.Number({ minimum: 0 });
// SDK catalog uses negative rates for unknown/dynamic pricing. Preserve them.
const costRates = { input: Type.Number(), output: Type.Number(), cacheRead: Type.Number(), cacheWrite: Type.Number() };
export const CatalogModelSchema = Type.Object({
  provider: text, id: text, name: text, api: text, baseUrl: Type.String(),
  input: Type.Array(Type.Union([Type.Literal("text"), Type.Literal("image")])),
  cost: Type.Object({ ...costRates, tiers: Type.Optional(Type.Array(
    Type.Object({ ...costRates, inputTokensAbove: nonnegative }, { additionalProperties: false }))) },
    { additionalProperties: false }),
  reasoning: Type.Boolean(), contextWindow: Type.Integer({ minimum: 1 }), maxTokens: Type.Integer({ minimum: 1 }),
}, { additionalProperties: false });
export const CatalogSchema = Type.Object({ models: Type.Array(CatalogModelSchema) }, { additionalProperties: false });
export type Catalog = Static<typeof CatalogSchema>;
export const ThinkingLevelSchema = Type.Union([
  Type.Literal("off"), Type.Literal("minimal"), Type.Literal("low"),
  Type.Literal("medium"), Type.Literal("high"), Type.Literal("xhigh"),
]);
export const PermissionModeSchema = Type.Union([
  Type.Literal("deny"), Type.Literal("manual"), Type.Literal("auto-current"), Type.Literal("auto-all"),
]);
export type PermissionMode = Static<typeof PermissionModeSchema>;
export const DecisionSchema = Type.Union([Type.Literal("allow"), Type.Literal("deny"), Type.Literal("allowAlways")]);
export const SessionOptionsSchema = Type.Object({
  model: Type.Optional(ModelSchema), thinkingLevel: Type.Optional(ThinkingLevelSchema),
  toolProfile: Type.Optional(text), permissionMode: Type.Optional(PermissionModeSchema),
  sandbox: Type.Optional(Type.Boolean()), expertPrompt: Type.Optional(Type.String()),
}, { additionalProperties: false });
export const ImageSchema = Type.Object({ type: Type.Literal("image"), mimeType: text, data: text },
  { additionalProperties: false });
export type SessionOptions = Static<typeof SessionOptionsSchema>;
export type ImageAttachment = Static<typeof ImageSchema>;
export const PingSchema = Type.Object({ status: Type.Literal("ok"), sessions: Type.Integer({ minimum: 0 }),
  usagePersistFailures: Type.Integer({ minimum: 0 }),
  experts: Type.Optional(Type.Object({ enabled: Type.Boolean(), prompt: Type.String() }, { additionalProperties: false })),
  version: text, piVersion: text, protocolVersion: Type.Literal(PROTOCOL_VERSION) });
// Keep every command explicit so unknown fields/methods cannot reach SDK APIs.
export const SandboxResultSchema = Type.Object({
  output: Type.String(), error: Type.Optional(text),
  exitCode: Type.Union([Type.Integer(), Type.Null()]),
  signal: Type.Union([Type.Integer({ minimum: 1 }), Type.Null()]), timedOut: Type.Boolean(),
});
export type SandboxResult = Static<typeof SandboxResultSchema>;
export const RequestSchema = Type.Union([
  request("provider.auth", { provider: text,
    operation: Type.Union([Type.Literal("start"), Type.Literal("poll"), Type.Literal("respond"), Type.Literal("cancel")]),
    input: Type.Optional(text) }),
  request("permission.respond", { sessionId: text, callId: text, decision: DecisionSchema }),
  request("sandbox.respond", { sessionId: text, callId: text, ...SandboxResultSchema.properties }),
  request("ping", {}), request("models.list", {}), request("models.catalog", {}),
  request("session.open", { sessionId: Type.Optional(text), cwd: Type.Optional(text), ...SessionOptionsSchema.properties }),
  request("session.configure", { sessionId: text, ...SessionOptionsSchema.properties }),
  request("prompt", { sessionId: text, text, attachments: Type.Optional(Type.Array(ImageSchema)) }),
  request("steer", { sessionId: text, text }),
  request("followUp", { sessionId: text, text }),
  request("abort", { sessionId: text }), request("close", { sessionId: text }),
  request("config.validate", { config: ConfigPatchSchema }),
  Type.Object({ id, method: Type.Literal("config.apply"), params: Type.Object({
    config: Type.Optional(ConfigPatchSchema), credentials: Type.Optional(CredentialsSchema),
  }, { additionalProperties: false }) }, { additionalProperties: false }),
]);
export const ErrorSchema = Type.Object({ code: text, message: Type.String(), paths: Type.Optional(Type.Array(Type.String())) });
export const ResponseSchema = Type.Union([
  Type.Object({ type: Type.Literal("response"), id, ok: Type.Literal(true), result: Type.Unknown() }),
  Type.Object({ type: Type.Literal("response"), id: Type.Union([id, Type.Null()]),
    ok: Type.Literal(false), error: ErrorSchema }),
]);
export type Request = Static<typeof RequestSchema>;
export type Response = Static<typeof ResponseSchema>;
