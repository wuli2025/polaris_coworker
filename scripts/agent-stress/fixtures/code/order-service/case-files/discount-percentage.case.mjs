import assert from "node:assert/strict";
import test from "node:test";
import { discountedTotal } from "../src/pricing.mjs";

test("treats 15 as a 15 percent discount", () => {
  assert.equal(discountedTotal([{ price: 1000, quantity: 2 }], 15), 1700);
});
