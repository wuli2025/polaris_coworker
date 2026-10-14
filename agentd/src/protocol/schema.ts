import { Type, type Static, type TSchema } from "typebox";
import { ConfigPatchSchema, CredentialsSchema } from "../config/schema.ts";
const text = Type.String({ minLength: 1 });
const id = Type.Union([text, Type.Integer()]);
const request = <M extends string, P extends Record<string, TSchema>>(method: M, params: P) =>
  Type.Object({ id, method: Type.Literal(method), params: Type.Object(params, { additionalProperties: false }) },
    { additionalProperties: false });
// Keep every command explicit so unknown fields/methods cannot reach SDK APIs.
export const RequestSchema = Type.Union([
  request("ping", {}), request("models.list", {}),
  request("session.open", { sessionId: Type.Optional(text), cwd: Type.Optional(text) }),
  request("prompt", { sessionId: text, text }),
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
