/** Safe operational failures, separate from permission decisions. Never carries
 * arbitrary exception text, paths, commands, or a permission feedback field. */
const messages = {
	unknown:
		"Guard operation failed; the cause is unknown. Inspect host diagnostics.",
	unavailable:
		"Guard service unavailable. Restore the connection before continuing.",
	cancelled:
		"Guard operation cancelled; inspect any in-flight effects before retrying.",
	invalid_request: "Invalid guard request.",
	invalid_binding: "Guard request binding changed or is invalid.",
	unknown_operator_method: "Unknown operator method.",
	tool_method_unavailable: "Method unavailable on the tool channel.",
	destination_busy:
		"Operator destination is stale, changing, or busy; command not started.",
	navigation_live:
		"Operator invocation or navigation is live; wait for confirmed completion.",
	receipt_capacity: "Operator receipt capacity exhausted; open a new Session.",
	receipt_unacknowledged:
		"Operator result was not acknowledged; effects must not be replayed. Reconnect and inspect /shell-results or /permissions.",
	receipt_unavailable:
		"Native operator receipt worker unavailable; command not started.",
	invalid_input_decision: "Input decision is stale or self-approved.",
	input_lease_owner: "Input lease belongs to another connection.",
	request_stale: "Request is no longer pending.",
	operation_revoked:
		"Operator invocation revoked; inspect effects before continuing.",
	result_unavailable:
		"Operator result unavailable; inspect effects before continuing.",
	receipt_bound: "Operator receipt exceeds its bound.",
	no_live_invocation: "No live invocation on this brain connection.",
	teardown_unconfirmed:
		"Invocation teardown not confirmed; inspect effects before continuing.",
	resource_session_only:
		"Resource access is granted per session, never always.",
} as const;
export type GuardErrorCode = keyof typeof messages;
export interface GuardOperationalFeedback {
	code: GuardErrorCode;
}
export function operationalFeedback(
	value: unknown,
): GuardOperationalFeedback | undefined {
	if (
		value === null ||
		typeof value !== "object" ||
		!("code" in value) ||
		typeof value.code !== "string" ||
		!Object.hasOwn(messages, value.code)
	)
		return undefined;
	return { code: value.code as GuardErrorCode };
}
export class GuardOperationalError extends Error {
	readonly operational: GuardOperationalFeedback;
	constructor(code: GuardErrorCode) {
		super(messages[code]);
		this.name = "GuardOperationalError";
		this.operational = { code };
	}
}
