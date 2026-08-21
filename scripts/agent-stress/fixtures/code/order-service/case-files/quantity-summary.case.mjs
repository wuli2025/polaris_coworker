import assert from "node:assert/strict";
import test from "node:test";
import { summarizeOrders } from "../src/orders.mjs";

test("summarizes quantity-weighted totals and order count", () => {
  assert.deepEqual(
    summarizeOrders([
      { items: [{ price: 100, quantity: 3 }] },
      { items: [{ price: 250, quantity: 2 }, { price: 20, quantity: 1 }] },
    ]),
    { count: 2, total: 820 },
  );
});
