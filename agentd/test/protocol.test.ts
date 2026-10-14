import assert from "node:assert/strict";
import { test } from "node:test";
import { Writable } from "node:stream";
import { Value } from "typebox/value";
import { JsonlReader, JsonlWriter } from "../src/protocol/jsonl.ts";
import { Redactor } from "../src/models/credentials.ts";
import { RequestSchema, ResponseSchema } from "../src/protocol/schema.ts";

function reader(limit = 100) {
  const values: unknown[] = [];
  const errors: string[] = [];
  return { values, errors, reader: new JsonlReader(() => limit,
    (v) => values.push(v), (e) => errors.push(e.code)) };
}
test("JSONL handles partial UTF-8, multiple lines, CRLF and unterminated EOF", () => {
  const f = reader();
  const bytes = Buffer.from('{"s":"中\u2028文"}\r\n1\n{"half":true}');
  f.reader.push(bytes.subarray(0, 7));
  assert.equal(f.values.length, 0);
  f.reader.push(bytes.subarray(7));
  f.reader.end();
  assert.deepEqual(f.values, [{ s: "中\u2028文" }, 1, { half: true }]);
  assert.deepEqual(f.errors, []);
});
test("oversized frames discard through LF, report once and recover", () => {
  const f = reader(8);
  f.reader.push(Buffer.from('"12345678'));
  f.reader.push(Buffer.from('90123456789"\n{}\n'));
  f.reader.end();
  assert.deepEqual(f.errors, ["LINE_TOO_LONG"]);
  assert.deepEqual(f.values, [{}]);
  const utf8 = reader(5);
  utf8.reader.push(Buffer.from('"中文"\n'));
  assert.deepEqual(utf8.errors, ["LINE_TOO_LONG"]);
});
test("invalid JSON/UTF-8 are named errors and do not poison following lines", () => {
  const f = reader();
  f.reader.push(Buffer.from('oops\n{"ok":true}\n'));
  f.reader.push(Buffer.from([0xff, 10]));
  assert.deepEqual(f.errors, ["INVALID_JSON", "INVALID_JSON"]);
  assert.deepEqual(f.values, [{ ok: true }]);
});
test("writer flushes complete lines in order despite backpressure and redacts nested secrets", async () => {
  const lines: string[] = [];
  const sink = new Writable({ highWaterMark: 1, write(chunk, _encoding, callback) {
    setTimeout(() => { lines.push(String(chunk)); callback(); }, 2);
  } });
  const redactor = new Redactor();
  redactor.add("super-secret");
  const writer = new JsonlWriter(sink, redactor);
  writer.send({ a: ["super-secret"] });
  writer.send({ b: 2 });
  await writer.flush();
  assert.deepEqual(lines, ['{"a":["[REDACTED]"]}\n', '{"b":2}\n']);
});
test("request and response schemas enforce correlation and command fields", () => {
  assert.ok(Value.Check(RequestSchema, { id: "r", method: "ping", params: {} }));
  assert.ok(!Value.Check(RequestSchema, { method: "ping", params: {} }));
  assert.ok(!Value.Check(RequestSchema, { id: 1, method: "wat", params: {} }));
  assert.ok(!Value.Check(RequestSchema, { id: 1, method: "ping", params: { key: "bad" } }));
  assert.ok(Value.Check(ResponseSchema, { type: "response", id: null, ok: false,
    error: { code: "INVALID_JSON", message: "Invalid JSON" } }));
});
