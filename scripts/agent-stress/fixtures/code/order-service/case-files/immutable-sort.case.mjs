import assert from "node:assert/strict";
import test from "node:test";
import { sortOrders } from "../src/orders.mjs";

test("sorts stably without mutating the input", () => {
  const input = [
    { id: "later", createdAt: "2026-02-01" },
    { id: "first-a", createdAt: "2026-01-01" },
    { id: "first-b", createdAt: "2026-01-01" },
  ];
  const snapshot = structuredClone(input);
  assert.deepEqual(sortOrders(input).map((order) => order.id), ["first-a", "first-b", "later"]);
  assert.deepEqual(input, snapshot);
});
