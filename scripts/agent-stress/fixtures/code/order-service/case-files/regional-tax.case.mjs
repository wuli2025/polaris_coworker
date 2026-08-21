import assert from "node:assert/strict";
import test from "node:test";
import { calculateTax } from "../src/pricing.mjs";

test("uses the declared regional tax rates", () => {
  assert.equal(calculateTax(1055, "JP"), 106);
  assert.equal(calculateTax(1055, "SG"), 95);
  assert.equal(calculateTax(1055, "US"), 0);
});
