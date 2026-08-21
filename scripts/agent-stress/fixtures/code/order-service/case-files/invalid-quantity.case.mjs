import assert from "node:assert/strict";
import test from "node:test";
import { validateLineItem } from "../src/orders.mjs";

test("accepts only positive integer quantities", () => {
  assert.equal(validateLineItem({ quantity: 2, price: 10 }), true);
  for (const quantity of [0, -1, 1.5]) {
    assert.equal(validateLineItem({ quantity, price: 10 }), false);
  }
});
