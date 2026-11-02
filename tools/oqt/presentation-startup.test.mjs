import assert from "node:assert/strict";
import test from "node:test";
import { createPresentationStartup } from "./presentation-startup.ts";

const target = {
	runtime: "runtime-a",
	session: "session-a",
	conversation: "conversation-a",
	model: "provider/model-a",
	host: "123:/run/operator.sock",
};
function setup(dispatch = async () => "accepted", extra = {}) {
	return createPresentationStartup({
		launchId: "launch-a",
		runtime: "runtime-a",
		dispatch,
		changed() {},
		...extra,
	});
}

test("queueing before readiness dispatches nothing until bound, then exactly once", async () => {
	const calls = [];
	const startup = setup(async (submission) => {
		calls.push(submission);
		return "accepted";
	});
	assert.equal(startup.input.submit("hello before startup"), true);
	assert.equal(startup.input.view().queued, true);
	assert.equal(calls.length, 0);
	await startup.ready(target);
	assert.equal(calls.length, 1);
	assert.equal(calls[0].text, "hello before startup");
	assert.deepEqual(calls[0].binding, target);
	assert.equal(calls[0].requestId, "launch-a:1");
	assert.equal(startup.input.view().status, "sent");
	assert.equal(startup.input.view().retained.length, 0);
});

test("only one startup message may be queued; blank text is not queued", () => {
	const startup = setup();
	assert.equal(startup.input.submit("   "), false);
	assert.equal(startup.input.submit("first"), true);
	assert.equal(startup.input.submit("second"), false);
	assert.equal(startup.input.view().queued, true);
});

test("cancel withdraws before dispatch and returns the text", async () => {
	let calls = 0;
	const startup = setup(async () => {
		calls++;
		return "accepted";
	});
	startup.input.submit("keep me");
	assert.equal(startup.input.cancel(), "keep me");
	assert.equal(startup.input.view().queued, false);
	assert.equal(startup.input.view().retained[0].text, "keep me");
	startup.input.release("launch-a:1");
	assert.equal(startup.input.view().retained.length, 0);
	await startup.ready(target);
	assert.equal(calls, 0);
	assert.equal(startup.input.view().status, "idle");
	assert.equal(startup.input.cancel(), undefined);
});

test("unbound, mismatched or incomplete readiness cannot release input", async () => {
	let calls = 0;
	const startup = createPresentationStartup({
		launchId: "launch-a",
		runtime: "runtime-a",
		expectedSession: "session-a",
		dispatch: async () => {
			calls++;
			return "accepted";
		},
		changed() {},
	});
	startup.input.submit("hello");
	for (const changed of [
		{ ...target, session: "other" },
		{ ...target, runtime: "other" },
		{ ...target, host: "" },
		{ ...target, model: undefined },
	])
		assert.throws(() => startup.ready(changed));
	assert.equal(calls, 0);
	assert.equal(startup.input.view().queued, true);
	await startup.ready(target);
	assert.equal(calls, 1);
	assert.throws(() => startup.ready(target));
});

test("startup failure retains queued text and blocks dispatch", () => {
	const startup = setup();
	startup.input.submit("hello");
	assert.equal(startup.fail(), "hello");
	assert.equal(startup.input.view().phase, "failed");
	assert.equal(startup.input.view().retained[0].text, "hello");
	assert.equal(startup.input.submit("again"), false);
	assert.throws(() => startup.ready(target));
});

test("rejected admission is retained, not retried", async () => {
	let calls = 0;
	const startup = setup(async () => {
		calls++;
		return "rejected";
	});
	startup.input.submit("not admitted");
	await startup.ready(target);
	assert.equal(calls, 1);
	assert.equal(startup.input.view().status, "rejected");
	assert.equal(startup.input.view().retained[0].text, "not admitted");
});

test("uncertain dispatch is retained and never automatically replayed", async () => {
	let calls = 0;
	const startup = setup(async () => {
		calls++;
		throw new Error("connection lost after admission");
	});
	startup.input.submit("exactly one attempt");
	await startup.ready(target);
	assert.equal(calls, 1);
	assert.equal(startup.input.view().status, "uncertain");
	assert.equal(startup.input.view().retained[0].text, "exactly one attempt");
	assert.equal(startup.input.submit("another"), false);
});

test("closure during dispatch fences the result and cannot reopen", async () => {
	let resolve;
	const pending = new Promise((done) => {
		resolve = done;
	});
	const startup = setup(async () => {
		await pending;
		return "rejected";
	});
	startup.input.submit("hello");
	const ready = startup.ready(target);
	assert.equal(startup.input.view().sending, true);
	startup.close();
	resolve();
	await ready;
	assert.equal(startup.input.view().phase, "closed");
	assert.equal(startup.input.view().status, "closed");
	assert.equal(startup.input.view().retained[0].text, "hello");
	assert.throws(() => startup.ready(target));
});

test("synchronous renderer closure before dispatch prevents dispatch", async () => {
	let calls = 0;
	const startup = setup(
		async () => {
			calls++;
			return "accepted";
		},
		{
			changed() {
				if (startup?.input.view().sending) startup.close();
			},
		},
	);
	startup.input.submit("hello");
	await startup.ready(target);
	assert.equal(calls, 0);
	assert.equal(startup.input.view().retained[0].text, "hello");
});

test("binding values are copied and immutable during dispatch", async () => {
	let captured;
	const mutable = { ...target };
	const startup = setup(async (submission) => {
		captured = submission;
		return "accepted";
	});
	startup.input.submit("hello");
	const ready = startup.ready(mutable);
	mutable.session = "forged";
	await ready;
	assert.equal(captured.binding.session, "session-a");
	assert.equal(Object.isFrozen(captured.binding), true);
});

test("renderer errors cannot replay or prevent admission", async () => {
	let calls = 0;
	const startup = setup(
		async () => {
			calls++;
			return "accepted";
		},
		{
			changed() {
				throw new Error("render failure");
			},
		},
	);
	startup.input.submit("hello");
	await startup.ready(target);
	assert.equal(calls, 1);
});

test("retained history is bounded", () => {
	const startup = setup();
	for (let index = 0; index < 70; index++) {
		startup.input.submit(`message ${index}`);
		startup.input.cancel();
	}
	assert.equal(startup.input.view().retained.length, 64);
	assert.equal(startup.input.view().retained.at(-1).text, "message 69");
});
