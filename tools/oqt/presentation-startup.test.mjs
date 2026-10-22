import assert from "node:assert/strict";
import test from "node:test";
import { createPresentationStartup } from "./presentation-startup.ts";

const target = { runtime: "runtime-a", session: "session-a", conversation: "conversation-a", model: "model-a", guardEpoch: "guard-a" };
function setup(dispatch = async () => "accepted", extra = {}) {
 return createPresentationStartup({ launchId: "launch-a", runtime: "runtime-a", expectedSession: "session-a", dispatch, changed() {}, ...extra });
}

test("typing and queueing work before readiness with zero dispatch", async () => {
 const calls = [];
 const controller = setup(async (submission) => { calls.push(submission); return "accepted"; });
 controller.input.edit("hello before startup");
 assert.equal(controller.input.view().draft, "hello before startup");
 assert.equal(controller.input.submit(), true);
 assert.equal(controller.input.view().queued, true);
 assert.equal(calls.length, 0);
 await controller.ready(target);
 assert.equal(calls.length, 1);
 assert.equal(calls[0].text, "hello before startup");
 assert.deepEqual(calls[0].binding, target);
 assert.equal(calls[0].requestId, "launch-a:1");
 assert.equal(controller.input.view().status, "ready");
});

test("cancelled startup input never dispatches and restores the draft", async () => {
 let calls = 0;
 const controller = setup(async () => { calls++; return "accepted"; });
 controller.input.edit("keep me"); controller.input.submit();
 assert.equal(controller.input.cancel(), "keep me");
 assert.equal(controller.input.view().draft, "keep me");
 await controller.ready(target);
 assert.equal(calls, 0);
});

test("cancelling returns queued text without overwriting a newer draft", () => {
 const controller = setup();
 controller.input.edit("queued"); controller.input.submit();
 controller.input.edit("newer");
 assert.equal(controller.input.cancel(), "queued");
 assert.equal(controller.input.view().draft, "newer");
});

test("unbound or mismatched readiness cannot release queued input", async () => {
 let calls = 0;
 const controller = setup(async () => { calls++; return "accepted"; });
 controller.input.edit("hello"); controller.input.submit();
 for (const changed of [{ ...target, session: "other" }, { ...target, runtime: "other" }, { ...target, guardEpoch: "" }]) {
 assert.throws(() => controller.ready(changed));
 }
 assert.equal(calls, 0);
 assert.equal(controller.input.view().queued, true);
 await controller.ready(target);
 assert.equal(calls, 1);
 assert.throws(() => controller.ready(target));
});

test("failure keeps the editor editable and preserves queued input", () => {
 const controller = setup();
 controller.input.edit("hello"); controller.input.submit();
 assert.equal(controller.fail(), "hello");
 assert.equal(controller.input.view().draft, "hello");
 controller.input.edit("edited after failure");
 assert.equal(controller.input.view().draft, "edited after failure");
 assert.equal(controller.input.submit(), false);
 assert.throws(() => controller.ready(target));
});

test("uncertain dispatch is not automatically replayed", async () => {
 let calls = 0;
 const controller = setup(async () => { calls++; throw new Error("connection lost after admission"); });
 controller.input.edit("exactly one attempt"); controller.input.submit();
 await controller.ready(target);
 assert.equal(calls, 1);
 assert.equal(controller.input.view().status, "uncertain");
 assert.equal(controller.input.view().draft, "exactly one attempt");
 assert.equal(controller.input.submit(), false);
});

test("closure fences an already pending dispatch and cannot reopen", async () => {
 let resolve;
 const pending = new Promise((done) => { resolve = done; });
 const controller = setup(async () => { await pending; return "accepted"; });
 controller.input.edit("hello"); controller.input.submit();
 const ready = controller.ready(target);
 assert.equal(controller.input.view().sending, true);
 controller.close(); resolve(); await ready;
 assert.equal(controller.input.view().phase, "closed");
 assert.equal(controller.input.view().status, "closed");
 assert.throws(() => controller.ready(target));
});

test("binding values are copied and immutable during dispatch", async () => {
 let captured;
 const mutable = { ...target };
 const controller = setup(async (submission) => { captured = submission; return "accepted"; });
 controller.input.edit("hello"); controller.input.submit();
 const ready = controller.ready(mutable);
 mutable.session = "forged";
 await ready;
 assert.equal(captured.binding.session, "session-a");
 assert.equal(Object.isFrozen(captured.binding), true);
});

test("rejected input survives alongside a newer draft", async () => {
 let release;
 const pending = new Promise((resolve) => { release = resolve; });
 const controller = setup(async () => { await pending; return "rejected"; });
 controller.input.edit("old queued text"); controller.input.submit();
 const ready = controller.ready(target);
 controller.input.edit("new draft");
 release(); await ready;
 assert.equal(controller.input.view().draft, "new draft");
 assert.equal(controller.input.view().retained[0].text, "old queued text");
});

test("synchronous renderer closure fences dispatch", async () => {
 let controller;
 let calls = 0;
 controller = setup(async () => { calls++; return "accepted"; }, { changed() {
 if (controller?.input.view().sending) controller.close();
 } });
 controller.input.edit("hello"); controller.input.submit();
 await controller.ready(target);
 assert.equal(calls, 0);
 assert.equal(controller.input.view().phase, "closed");
});

test("renderer errors cannot replay or prevent admission", async () => {
 let calls = 0;
 const controller = setup(async () => { calls++; return "accepted"; }, { changed() { throw new Error("render failure"); } });
 controller.input.edit("hello"); controller.input.submit();
 await controller.ready(target);
 assert.equal(calls, 1);
});
