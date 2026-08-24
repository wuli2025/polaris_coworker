import assert from "node:assert/strict";
import test from "node:test";
import { nextOrderId } from "../src/orders.mjs";

test("increments the greatest existing numeric order id", () => {
  assert.equal(nextOrderId([{ id: "ORD-0002" }, { id: "ORD-0041" }, { id: "legacy" }]), "ORD-0042");
});
