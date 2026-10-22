import assert from "node:assert/strict";
import test from "node:test";
import { PERMISSION_CODES, permissionFeedback, permissionFeedbackText } from "./permission-feedback.ts";

test("permission feedback attributes only explicit authority codes, never error prose", () => {
	for (const message of ["Permission denied", "operator denied", "Access not granted", "secret-token\u001b[2J"])
		assert.equal(permissionFeedback(new Error(message)).code, "unknown");
	for (const code of PERMISSION_CODES) {
		const view = permissionFeedback({ feedback: { code } });
		assert.equal(view.code, code);
		assert.equal(view.schema, "oq.permission-feedback.v1");
		assert.equal(view.execution, "not_started");
		assert.ok(view.title && view.message && view.guidance);
		assert.ok(!permissionFeedbackText(view).includes("secret-token"));
		assert.ok(view.actions.every((action) => ["review_permissions", "explain_scope"].includes(action.id)));
	}
});

test("typed transport lifecycle is preserved without classifying administrative errors as declines", () => {
	for (const code of ["unavailable", "cancelled"])
		assert.equal(permissionFeedback({ operational: { code } }).code, code);
	assert.equal(permissionFeedback({ operational: { code: "receipt_capacity" } }).code, "unknown");
	assert.equal(permissionFeedback({ operational: { code: "operator_denied" } }).code, "unknown");
});

test("safety and configured blocks tell the user what can change without suggesting approval override", () => {
	const safety = permissionFeedback({ feedback: { code: "protected_safety" } });
	assert.match(safety.message, /Ordinary approval cannot override/);
	assert.match(safety.guidance, /specific unprotected subfolder/);
	assert.match(safety.guidance, /covers its tree/);
	assert.doesNotMatch(safety.title, /Declined/);
	const configured = permissionFeedback({ feedback: { code: "configured_ceiling" } });
	assert.match(configured.guidance, /policy owner/);
	assert.match(configured.guidance, /cannot widen the owner ceiling/);
	assert.deepEqual(safety.actions.map((value) => value.id), ["review_permissions", "explain_scope"]);
});

test("uncertain effects get inspection guidance, never automatic retry or an unstarted claim", () => {
	const view = permissionFeedback(undefined, "uncertain");
	assert.match(view.message, /Effects may already have occurred/);
	assert.match(view.guidance, /Inspect/);
	assert.match(permissionFeedbackText(view), /outcome is uncertain/);
	assert.doesNotMatch(permissionFeedbackText(view), /was not started/);
	assert.deepEqual(view.actions.map((value) => value.id), ["review_permissions"]);
});

test("unknown, hostile and malformed feedback cannot leak error contents", () => {
	const hostile = { get feedback() { throw new Error("private-owner-key"); } };
	for (const error of [hostile, { feedback: null }, { feedback: { code: "approve_everything" } }, null]) {
		const view = permissionFeedback(error);
		assert.equal(view.code, "unknown");
		assert.doesNotMatch(permissionFeedbackText(view), /private-owner-key|approve_everything/);
	}
});
