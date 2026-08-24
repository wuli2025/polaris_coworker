import assert from "node:assert/strict";
import test from "node:test";
import { groupOrdersByCustomer } from "../src/orders.mjs";

test("groups customer names without case or surrounding whitespace", () => {
  const groups = groupOrdersByCustomer([
    { id: "1", customer: " Alice " },
    { id: "2", customer: "alice" },
    { id: "3", customer: "BOB" },
  ]);
  assert.deepEqual(Object.keys(groups).sort(), ["alice", "bob"]);
  assert.deepEqual(groups.alice.map((order) => order.id), ["1", "2"]);
});
