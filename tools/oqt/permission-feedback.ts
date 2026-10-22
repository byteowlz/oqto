// Host-neutral permission feedback. Codes come from admission, never error-text parsing.
export const PERMISSION_CODES = [
	"protected_safety", "configured_ceiling", "operator_denied", "expired",
	"stale", "cancelled", "unavailable", "unknown",
] as const;
export type PermissionCode = (typeof PERMISSION_CODES)[number];
export type PermissionAction = {
	id: "review_permissions" | "explain_scope";
	label: string;
};
export type PermissionFeedbackView = {
	schema: "oq.permission-feedback.v1";
	code: PermissionCode;
	execution: "not_started" | "uncertain";
	title: string;
	message: string;
	guidance: string;
	actions: PermissionAction[];
};

const COPY: Record<PermissionCode, [string, string, string]> = {
	protected_safety: [
		"Blocked by a safety rule",
		"This scope overlaps protected locations or is broader than the workspace permits. Ordinary approval cannot override this rule.",
		"Request a specific unprotected subfolder instead of a parent such as your home directory. Directory read access covers its tree, not only folder names.",
	],
	configured_ceiling: [
		"Blocked by configured policy",
		"The requested access is outside the configured permission ceiling. No approval can grant it under the current policy.",
		"Choose a narrower permitted scope, or ask the policy owner to review the relevant sandbox.toml rule. Workspace policy cannot widen the owner ceiling.",
	],
	operator_denied: [
		"Declined by the operator",
		"The operator declined this request. No new access was granted.",
		"Review the scope with the operator. Request a narrower scope if appropriate; do not bypass or automatically repeat the declined request.",
	],
	expired: [
		"Authorization expired",
		"The request or its authorization is no longer valid.",
		"Review current access with the operator before making a fresh request. An expired request is not an operator decline.",
	],
	stale: [
		"Request changed",
		"The request no longer matches the current Session or permission state.",
		"Refresh the permission view and review the current request. Do not apply a decision to an older revision.",
	],
	cancelled: [
		"Request cancelled",
		"This request was cancelled; no new approval was confirmed.",
		"Check whether the request is still needed. A cancellation is not an operator decline.",
	],
	unavailable: [
		"Approval unavailable",
		"The approval channel could not confirm authorization. This does not mean the operator declined.",
		"Reconnect the operator client and review pending requests and current grants before making another attempt.",
	],
	unknown: [
		"Authorization not confirmed",
		"This result does not confirm new authorization; its precise cause was not established. Existing grants may still be active. Do not assume the operator declined.",
		"Review current grants and connection status with the operator. Do not bypass the restriction.",
	],
};

export function permissionCode(value: unknown): PermissionCode {
	if (typeof value === "string" && (PERMISSION_CODES as readonly string[]).includes(value))
		return value as PermissionCode;
	return "unknown";
}

export function permissionFeedback(
	error: unknown,
	execution: "not_started" | "uncertain" = "not_started",
): PermissionFeedbackView {
	let code: PermissionCode = "unknown";
	try {
		if (error && typeof error === "object" && "feedback" in error) {
			const feedback = error.feedback;
			if (feedback && typeof feedback === "object" && "code" in feedback)
				code = permissionCode(feedback.code);
		} else if (error && typeof error === "object" && "operational" in error) {
			// Transport lifecycle is semantic too; administrative errors are not declines.
			const operational = error.operational;
			if (operational && typeof operational === "object" && "code" in operational &&
				(operational.code === "unavailable" || operational.code === "cancelled"))
				code = operational.code;
		}
	} catch { /* Unrecognized failures never become an attributed decline. */ }
	const [title, message, guidance] = COPY[code];
	return {
		schema: "oq.permission-feedback.v1", code, execution,
		title: execution === "uncertain" ? "Operation interrupted or authorization ended" : title,
		message: execution === "uncertain"
			? "The operation was admitted before it failed or lost authorization. Effects may already have occurred."
			: message,
		guidance: execution === "uncertain"
			? "Inspect the affected state before retrying. Do not automatically repeat a potentially completed effect."
			: guidance,
		actions: [
			{ id: "review_permissions", label: "Review permissions" },
			...(execution === "not_started" && (code === "protected_safety" || code === "configured_ceiling")
				? [{ id: "explain_scope" as const, label: "Understand this scope" }]
				: []),
		],
	};
}

export function permissionFeedbackText(value: PermissionFeedbackView): string {
	return `${value.title}\n${value.message}\nWhat to do: ${value.guidance}\n${value.execution === "not_started" ? "The confined operation was not started." : "Execution outcome is uncertain."}\nUse /permission-help for supported review actions, or ask your operator to review the request.`;
}
