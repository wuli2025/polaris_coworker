import { Type, type Static, type TSchema } from "typebox";
import { ConfigPatchSchema, CredentialsSchema } from "../config/schema.ts";
const text = Type.String({ minLength: 1 });
const id = Type.Union([text, Type.Integer()]);
const request = <M extends string, P extends Record<string, TSchema>>(method: M, params: P) =>
  Type.Object({ id, method: Type.Literal(method), params: Type.Object(params, { additionalProperties: false }) },
    { additionalProperties: false });
export const PROTOCOL_VERSION = 2;
export const ModelSchema = Type.Object({ provider: text, id: text }, { additionalProperties: false });
export const ThinkingLevelSchema = Type.Union([
  Type.Literal("off"), Type.Literal("minimal"), Type.Literal("low"),
  Type.Literal("medium"), Type.Literal("high"), Type.Literal("xhigh"),
]);
export const SessionOptionsSchema = Type.Object({
  model: Type.Optional(ModelSchema), thinkingLevel: Type.Optional(ThinkingLevelSchema),
  toolProfile: Type.Optional(text),
}, { additionalProperties: false });
export const ImageSchema = Type.Object({ type: Type.Literal("image"), mimeType: text, data: text },
  { additionalProperties: false });
export type SessionOptions = Static<typeof SessionOptionsSchema>;
export type ImageAttachment = Static<typeof ImageSchema>;
export const PingSchema = Type.Object({ status: Type.Literal("ok"), sessions: Type.Integer({ minimum: 0 }),
  version: text, piVersion: text, protocolVersion: Type.Literal(PROTOCOL_VERSION) });
// Keep every command explicit so unknown fields/methods cannot reach SDK APIs.
export const RequestSchema = Type.Union([
  request("ping", {}), request("models.list", {}),
  request("session.open", { sessionId: Type.Optional(text), cwd: Type.Optional(text), ...SessionOptionsSchema.properties }),
  request("session.configure", { sessionId: text, ...SessionOptionsSchema.properties }),
  request("prompt", { sessionId: text, text, attachments: Type.Optional(Type.Array(ImageSchema)) }),
  request("steer", { sessionId: text, text }),
  request("followUp", { sessionId: text, text }),
  request("abort", { sessionId: text }), request("close", { sessionId: text }),
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
