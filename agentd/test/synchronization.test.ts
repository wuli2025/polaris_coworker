import assert from "node:assert/strict";
import { test } from "node:test";
import { Changes, waitBudget } from "./synchronization.ts";

test("event waits observe existing state, later changes and premature process exit", async () => {
  const changes = new Changes();
  let ready = false;
  const waiting = changes.wait(() => ready, "ready");
  ready = true;
  changes.notify();
  await waiting;
  await changes.wait(() => ready, "already ready");
  const failed = assert.rejects(changes.wait(() => false, "missing response"), /exited early/);
  changes.close(new Error("exited early"));
  await failed;
  await changes.wait(() => ready, "buffered final event after exit");
});

test("event wait deadline fails closed and budget rejects invalid overrides", async (t) => {
  t.mock.timers.enable({ apis: ["setTimeout"] });
  const changes = new Changes();
  const failed = assert.rejects(changes.wait(() => false, "missing ready", 1000), /missing ready/);
  t.mock.timers.tick(1000);
  await failed;
  assert.equal(waitBudget("60000"), 60000);
  for (const value of ["", "0", "-1", "1.5", "NaN", "Infinity", "300001"]) {
    assert.throws(() => waitBudget(value), /POLARIS_TEST_WAIT_MS/);
  }
});
