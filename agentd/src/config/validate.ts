import { Value } from "typebox/value";
import type { TSchema } from "typebox";
import { DaemonError } from "../protocol/errors.ts";

/** JSON Pointer paths only: TypeBox messages/values must never cross the protocol. */
export function validateConfig(schema: TSchema, value: unknown): void {
  const paths = new Set<string>();
  for (const error of Value.Errors(schema, value)) {
    const keys = error.keyword === "additionalProperties" ? error.params.additionalProperties
      : error.keyword === "required" ? error.params.requiredProperties : undefined;
    if (keys) {
      for (const key of keys) paths.add(`${error.instancePath}/${key.replace(/~/g, "~0").replace(/\//g, "~1")}`);
    } else paths.add(error.instancePath || "/");
  }
  if (paths.size) throw new DaemonError("INVALID_CONFIG", "配置字段校验失败。", [...paths].sort());
}
