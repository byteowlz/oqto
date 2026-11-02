import assert from "node:assert/strict";
import { mkdtemp, rm } from "node:fs/promises";
import test from "node:test";
import { PermissionDeniedError } from "./coding-authority.ts";
import { GuardOperationalError, operationalFeedback } from "./guard-errors.ts";
import { MAX_FRAME, serve, WireClient } from "./guard-wire.ts";

async function fixture(handler, run) {
	const root = await mkdtemp("/tmp/oqt-wire-feedback-");
	const path = `${root}/sock`;
	const server = await serve(path, handler);
	const client = await WireClient.connect(path);
	try {
		await run(client);
	} finally {
		client.close();
		await new Promise((resolve) => server.close(resolve));
		await rm(root, { recursive: true, force: true });
	}
}
const operational = (code) => (error) => {
	assert.ok(error instanceof GuardOperationalError);
	assert.deepEqual(error.operational, { code });
	assert.equal("feedback" in error, false);
	assert.doesNotMatch(error.message, /private|credential|operator declined/);
	return true;
};
test("wire preserves authoritative permission codes without private fields", async () => {
	for (const code of [
		"protected_safety",
		"configured_ceiling",
		"operator_denied",
		"expired",
		"stale",
		"cancelled",
		"unavailable",
		"unknown",
	]) {
		await fixture(
			async () => {
				throw Object.assign(new Error("/private/credential"), {
					feedback: { code, root: "/private/credential" },
				});
			},
			async (client) => {
				await assert.rejects(client.call("prepare"), (error) => {
					assert.ok(error instanceof PermissionDeniedError);
					assert.deepEqual(error.feedback, { code });
					assert.doesNotMatch(
						error.message + JSON.stringify(error),
						/credential/,
					);
					return true;
				});
			},
		);
	}
});
test("wire operational errors preserve safe codes, not arbitrary error bodies", async () => {
	await fixture(
		(method) => {
			if (method === "capacity")
				throw new GuardOperationalError("receipt_capacity");
			if (method === "typed")
				throw Object.assign(new Error("/private/credential"), {
					operational: { code: "unknown_operator_method", root: "/private" },
				});
			if (method === "forged")
				throw Object.assign(new Error("operator denied /private/credential"), {
					feedback: { code: "invented" },
					operational: { code: "invented" },
				});
			throw new Error("operator denied /private/credential");
		},
		async (client) => {
			await assert.rejects(
				client.call("capacity"),
				operational("receipt_capacity"),
			);
			await assert.rejects(
				client.call("typed"),
				operational("unknown_operator_method"),
			);
			await assert.rejects(client.call("forged"), operational("unknown"));
			await assert.rejects(client.call("generic"), operational("unknown"));
		},
	);
});
test("client validates raw wire enum and ignores remote message and extra fields", async () => {
	for (const code of ["receipt_capacity", "invented", "constructor"]) {
		await fixture(
			(_method, _params, _signal, peer) => {
				peer.socket.write(
					`${JSON.stringify({ id: 1, ok: false, error: "operator denied /private/credential", operationalError: { code, root: "/private" } })}\n`,
				);
				return new Promise(() => {});
			},
			async (client) => {
				await assert.rejects(
					client.call("raw"),
					operational(code === "receipt_capacity" ? code : "unknown"),
				);
			},
		);
	}
});
test("operational enum validation rejects inherited keys and discards details", () => {
	for (const code of [
		"__proto__",
		"constructor",
		"toString",
		"operator_denied",
		"invented",
	])
		assert.equal(operationalFeedback({ code }), undefined);
	assert.deepEqual(
		operationalFeedback({ code: "receipt_capacity", root: "/private" }),
		{ code: "receipt_capacity" },
	);
});
test("wire disconnect and closed call are operational unavailable", () =>
	fixture(
		(_method, _params, _signal, peer) => {
			peer.socket.destroy();
			return new Promise(() => {});
		},
		async (client) => {
			await assert.rejects(
				client.call("disconnect"),
				operational("unavailable"),
			);
			await assert.rejects(client.call("closed"), operational("unavailable"));
		},
	));
test("wire cancellation is operational, not a recorded permission denial", () =>
	fixture(
		async (_method, _params, signal) =>
			new Promise((resolve) =>
				signal.addEventListener("abort", () => resolve(true), { once: true }),
			),
		async (client) => {
			const already = new AbortController();
			already.abort(new Error("operator denied"));
			await assert.rejects(
				client.call("wait", {}, already.signal),
				operational("cancelled"),
			);
			const active = new AbortController();
			const waiting = client.call("wait", {}, active.signal);
			active.abort(new Error("operator denied /private/credential"));
			await assert.rejects(waiting, operational("cancelled"));
		},
	));
test("wire connection failure and oversized outbound frames are operational", async () => {
	await assert.rejects(
		WireClient.connect("/tmp/oqt-nonexistent-feedback.sock", 0),
		operational("unavailable"),
	);
	await fixture(
		async () => true,
		async (client) => {
			await assert.rejects(
				client.call("oversized", { body: "x".repeat(MAX_FRAME) }),
				operational("invalid_request"),
			);
		},
	);
});
