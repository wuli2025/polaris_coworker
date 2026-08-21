import assert from "node:assert/strict";
import test from "node:test";
import { addOrder } from "../src/orders.mjs";

test("returns a new order list without mutating the caller array", () => {
  const existing = [{ id: "ORD-0001" }];
  const result = addOrder(existing, { id: "ORD-0002" });
  assert.deepEqual(existing, [{ id: "ORD-0001" }]);
  assert.deepEqual(result, [{ id: "ORD-0001" }, { id: "ORD-0002" }]);
  assert.notEqual(result, existing);
});
