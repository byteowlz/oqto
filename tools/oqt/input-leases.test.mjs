import assert from "node:assert/strict";
import test from "node:test";
import { SessionInputLeases } from "./input-leases.ts";

const call = (member = "prompt", args = []) => ({
	serviceId: "pi.agent-controller",
	member,
	args,
});
function peer() {
	const abort = new AbortController();
	return { closed: abort.signal, abort, socket: {}, push() {} };
}
function fixture(options = {}) {
	const cancelled = [];
	let stopped = 0;
	const leases = new SessionInputLeases(() => {}, options.duration ?? 60000);
	const cancel = async (id) => {
		if (options.cancelFails) throw Error("fixture lost");
		cancelled.push(id);
	};
	const stop = async () => {
		stopped++;
		if (options.stopFails) throw Error("fixture stop unknown");
	};
	for (const id of ["local", "remote-a", "remote-b"])
		leases.register(id, cancel, stop);
	const actor = peer(),
		terminal = peer();
	return {
		leases,
		cancelled,
		stopped: () => stopped,
		actor,
		terminal,
		async acquire(attachment = "remote-a", requester = actor) {
			const waiting = leases.request(
				requester,
				attachment,
				"Unverified fixture controller",
				requester.closed,
			);
			const id = leases.view().pending?.id;
			assert.ok(id);
			await leases.decide(terminal, id, true);
			assert.equal((await waiting).granted, true);
		},
	};
}
test("remote input requires a distinct terminal decision and cannot borrow another attachment", async () => {
	const f = fixture();
	try {
		const pending = f.leases.request(
			f.actor,
			"remote-a",
			"fixture",
			f.actor.closed,
		);
		await assert.rejects(
			f.leases.decide(f.actor, f.leases.view().pending.id, true),
			/self-approved/,
		);
		await f.leases.decide(f.terminal, f.leases.view().pending.id, true);
		assert.equal((await pending).granted, true);
		const b = peer();
		assert.equal(
			(await f.leases.request(b, "remote-b", "fixture", b.closed)).granted,
			false,
		);
		await assert.rejects(f.leases.before("remote-b", call()), /lease required/);
		await f.leases.before("remote-a", call());
		assert.throws(() => f.leases.release(b), /another connection/);
	} finally {
		await f.leases.close();
	}
});
test("native terminal input cancels only recorded remote native submission IDs", async () => {
	const f = fixture();
	try {
		await f.acquire();
		const admission = await f.leases.before("remote-a", call());
		await f.leases.after(admission, call(), {
			accepted: true,
			operationId: "42",
		});
		await f.leases.before("local", call());
		assert.equal(f.leases.view().holder, null);
		assert.deepEqual(f.cancelled, ["42"]);
		await assert.rejects(f.leases.before("remote-a", call()), /lease required/);
		assert.equal(f.stopped(), 0);
	} finally {
		await f.leases.close();
	}
});
test("revocation racing a native accepted receipt cancels that exact late submission", async () => {
	const f = fixture();
	try {
		await f.acquire();
		const admission = await f.leases.before("remote-a", call("steer"));
		f.leases.disconnect(f.actor);
		await f.leases.after(admission, call("steer"), {
			accepted: true,
			entryId: "43",
		});
		assert.deepEqual(f.cancelled, ["43"]);
		assert.equal(f.leases.view().holder, null);
	} finally {
		await f.leases.close();
	}
});
test("a cancelled or detached pending request cannot be approved later", async () => {
	for (const action of ["abort", "detach", "terminal"]) {
		const f = fixture();
		const pending = f.leases.request(
			f.actor,
			"remote-a",
			"fixture",
			f.actor.closed,
		);
		const id = f.leases.view().pending.id;
		if (action === "abort") f.actor.abort.abort();
		else if (action === "detach") await f.leases.unregister("remote-a");
		else await f.leases.before("local", call());
		assert.equal((await pending).granted, false);
		await assert.rejects(f.leases.decide(f.terminal, id, true));
		await f.leases.close();
	}
});
test("cancel failure stops only the registered owned worker; failed stop freezes input", async () => {
	for (const stopFails of [false, true]) {
		const f = fixture({ cancelFails: true, stopFails });
		try {
			await f.acquire();
			const admission = await f.leases.before("remote-a", call());
			await f.leases.after(admission, call(), {
				accepted: true,
				operationId: "44",
			});
			if (stopFails) await assert.rejects(f.leases.before("local", call()));
			else await f.leases.before("local", call());
			assert.equal(f.stopped(), 1);
			assert.equal(f.leases.view().available, !stopFails);
		} finally {
			await f.leases.close();
		}
	}
});
test("input expiry cancels queued remote input without another client request", async () => {
	const f = fixture({ duration: 5 });
	try {
		await f.acquire();
		const admission = await f.leases.before("remote-a", call());
		await f.leases.after(admission, call(), {
			accepted: true,
			operationId: "45",
		});
		await new Promise((r) => setTimeout(r, 20));
		assert.equal(f.leases.view().holder, null);
		assert.deepEqual(f.cancelled, ["45"]);
	} finally {
		await f.leases.close();
	}
});
test("dispatch fence rejects revoke after awaited admission, without stopping the worker", async () => {
	const f = fixture();
	try {
		await f.acquire();
		const admission = await f.leases.before("remote-a", call());
		f.leases.assertCurrent(admission);
		f.leases.release(f.actor);
		assert.throws(
			() => f.leases.assertCurrent(admission),
			/changed before dispatch/,
		);
		assert.equal(f.stopped(), 0);
	} finally {
		await f.leases.close();
	}
});
test("a remote lease cannot change models, compact, follow up, or cancel another source", async () => {
	const f = fixture();
	try {
		await f.acquire();
		for (const member of ["compact", "followUp"])
			await assert.rejects(
				f.leases.before("remote-a", call(member)),
				/unsupported/,
			);
		await assert.rejects(
			f.leases.before("remote-a", call("cancelQueued", ["999"])),
			/another source/,
		);
		await assert.rejects(
			f.leases.before("remote-a", {
				serviceId: "pi.models",
				member: "select",
				args: [],
			}),
			/unsupported/,
		);
		await f.leases.before("local", {
			serviceId: "pi.models",
			member: "select",
			args: [],
		});
		assert.equal(f.leases.view().holder, null);
	} finally {
		await f.leases.close();
	}
});
test("a remote grant racing terminal admission invalidates local dispatch", async () => {
	const f = fixture();
	try {
		const local = f.leases.before("local", call());
		const request = f.leases.request(
			f.actor,
			"remote-a",
			"fixture",
			f.actor.closed,
		);
		const decision = f.leases.decide(
			f.terminal,
			f.leases.view().pending.id,
			true,
		);
		const admission = await local;
		await decision;
		assert.equal((await request).granted, true);
		assert.equal(f.leases.view().holder, "remote-a");
		assert.throws(
			() => f.leases.assertCurrent(admission),
			/changed before dispatch/,
		);
	} finally {
		await f.leases.close();
	}
});
test("conversation navigation is local control, never inherited from a remote input lease", async () => {
	const f = fixture();
	const navigation = (member) => ({
		serviceId: "oqt.session-forks",
		member,
		args: [],
	});
	try {
		await f.acquire();
		const input = await f.leases.before("remote-a", call());
		await f.leases.after(input, call(), { accepted: true, operationId: "42" });
		for (const member of ["select", "fork"])
			await assert.rejects(
				f.leases.before("remote-a", navigation(member)),
				/unsupported/,
			);
		for (const member of ["selected", "conversations", "entries"])
			assert.equal(
				await f.leases.before("local", navigation(member)),
				undefined,
			);
		assert.equal(f.leases.view().holder, "remote-a");
		const local = await f.leases.before("local", navigation("select"));
		f.leases.assertCurrent(local);
		assert.equal(f.leases.view().holder, null);
		assert.deepEqual(f.cancelled, ["42"]);
		await assert.rejects(
			f.leases.before("remote-a", navigation("fork")),
			/unsupported/,
		);
	} finally {
		await f.leases.close();
	}
});

test("observation does not reclaim input or grant any tool decision", async () => {
	const f = fixture();
	try {
		await f.acquire();
		assert.equal(
			await f.leases.before("local", call("waitForPrompt")),
			undefined,
		);
		assert.equal(
			await f.leases.before("local", {
				serviceId: "pi.transcript",
				member: "subscribe",
				args: [],
			}),
			undefined,
		);
		assert.equal(f.leases.view().holder, "remote-a");
		assert.equal("decideTool" in f.leases, false);
	} finally {
		await f.leases.close();
	}
});
test("recovered native worker never inherits retired attachment authority", async () => {
	const f = fixture();
	try {
		await f.acquire();
		f.leases.forget("remote-a");
		await assert.rejects(f.leases.before("remote-a", call()), /unavailable/);
		const next = await f.leases.request(
			f.actor,
			"remote-a",
			"fixture",
			f.actor.closed,
		);
		assert.equal(next.granted, false);
	} finally {
		await f.leases.close();
	}
});
