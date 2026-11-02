import assert from "node:assert/strict";
import test from "node:test";
import { CodingAuthority, PermissionDeniedError } from "./coding-authority.ts";
import { GuardOperationalError } from "./guard-errors.ts";

const operation = {
	request_id: "session:store:test",
	target: { capability: "file_read", resource_id: "path:/home" },
	arguments: {},
};
async function authorityFixture(run) {
	const previous = process.env.OQT_CARRIER;
	// Calls are stubbed; the constructor's unused operator process is inert.
	process.env.OQT_CARRIER = "/bin/true";
	let authority;
	try {
		authority = new CodingAuthority("session", "/tmp", {}, () => {}, {});
		await run(authority);
	} finally {
		await authority?.operator.close();
		if (previous === undefined) delete process.env.OQT_CARRIER;
		else process.env.OQT_CARRIER = previous;
	}
}
for (const code of [
	"protected_safety",
	"configured_ceiling",
	"operator_denied",
	"expired",
	"stale",
	"unknown",
]) {
	test(`authority preserves ${code} without arbitrary fields`, () =>
		authorityFixture(async (authority) => {
			authority.tools = {
				call: async () => ({
					admission: {
						status: "denied",
						value: { code, root: "/private/credential" },
					},
				}),
			};
			await assert.rejects(
				authority.prepare(operation, new AbortController().signal),
				(error) => {
					assert.ok(error instanceof PermissionDeniedError);
					assert.deepEqual(error.feedback, { code });
					assert.doesNotMatch(JSON.stringify(error), /credential/);
					assert.doesNotMatch(error.message, /credential/);
					return true;
				},
			);
		}));
}
test("authority transport failure and cancellation never infer operator denial", () =>
	authorityFixture(async (authority) => {
		authority.tools = {
			call: async () => {
				throw new Error("operator denied /private/credential");
			},
		};
		await assert.rejects(
			authority.prepare(operation, new AbortController().signal),
			(error) =>
				error.feedback.code === "unavailable" &&
				!error.message.includes("credential"),
		);
		const abort = new AbortController();
		abort.abort();
		await assert.rejects(
			authority.prepare(operation, abort.signal),
			(error) => error.feedback.code === "cancelled",
		);
	}));
test("malformed authority replies have unknown attribution", () =>
	authorityFixture(async (authority) => {
		for (const result of [
			null,
			{},
			{ admission: { status: "denied", value: { code: "invented" } } },
		]) {
			authority.tools = { call: async () => result };
			await assert.rejects(
				authority.prepare(operation, new AbortController().signal),
				(error) => error.feedback.code === "unknown",
			);
		}
	}));
test("operator capacity remains operational, never a permission denial", () =>
	authorityFixture(async (authority) => {
		authority.operator.call = async () => ({
			operational_error: {
				code: "receipt_capacity",
				root: "/private/credential",
			},
		});
		await assert.rejects(
			authority.operatorBegin("session:operator:quota", "private command"),
			(error) => {
				assert.ok(error instanceof GuardOperationalError);
				assert.deepEqual(error.operational, { code: "receipt_capacity" });
				assert.equal("feedback" in error, false);
				assert.doesNotMatch(
					error.message + JSON.stringify(error),
					/credential|private command/,
				);
				return true;
			},
		);
	}));
test("delivery preserves lifecycle feedback without arbitrary details", () =>
	authorityFixture(async (authority) => {
		for (const code of ["expired", "stale"]) {
			authority.tools = {
				call: async () => ({
					checked: false,
					feedback: { code, root: "/private/credential" },
				}),
			};
			await assert.rejects(authority.check(operation), (error) => {
				assert.deepEqual(error.feedback, { code });
				assert.doesNotMatch(JSON.stringify(error), /credential/);
				return true;
			});
		}
	}));
