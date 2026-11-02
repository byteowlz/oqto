import type {
	PresentationUI,
	SlashCommandContribution,
	SlashCommands,
	ToolPresentationInput,
	ToolPresentationSummary,
} from "@earendil-works/pi-coding-agent/experimental/plugin";
import { permissionFeedback } from "../../../../src/experimental/permission-feedback.ts";

/** Supported native selector fallback; closed intents, never eval/shell/retry. */
export function permissionHelpCommand(
	commands: SlashCommands,
	ui: PresentationUI,
): SlashCommandContribution {
	return {
		name: "permission-help",
		description:
			"Explain access outcomes and review permissions (never approve or retry)",
		async run(_args, context) {
			const intent = await ui.select(
				"Access request help · no automatic approval or retry",
				[
					{
						value: "explain_scope",
						label: "Explain request scope and blocked outcomes",
					},
					{
						value: "review_permissions",
						label: "Open /permissions for user review",
					},
				],
				"explain_scope",
				context,
			);
			if (intent === "review_permissions") {
				const review = commands
					.list()
					.find((command) => command.name === "permissions");
				if (review) return review.run("", context);
				ui.showStatus(
					"Permission review unavailable; no action taken",
					context,
				);
			} else if (intent === "explain_scope")
				await ui.select(
					"Directory reads cover the entire tree, not just names. Protected safety limits cannot be approved away. Configured policy differs from operator decline. Unknown, expired or unavailable outcomes do not confirm new authorization. Review current grants with /permissions; nothing is retried automatically.",
					[{ value: "close", label: "Close explanation" }],
					"close",
					context,
				);
			return undefined;
		},
	};
}

function object(value: unknown): Record<string, unknown> {
	return value !== null && typeof value === "object" && !Array.isArray(value)
		? (value as Record<string, unknown>)
		: {};
}

/** Escape rather than strip: distinct paths must remain distinct to the operator. */
export function permissionDisplayText(value: string): string {
	return Array.from(value, (character) => {
		const code = character.codePointAt(0) ?? 0;
		return code < 32 ||
			(code >= 127 && code <= 159) ||
			/[\p{Cf}\p{Zl}\p{Zp}]/u.test(character)
			? `\\u${code.toString(16).padStart(4, "0")}`
			: character;
	}).join("");
}

export function renderPermission(
	input: ToolPresentationInput,
): ToolPresentationSummary {
	const args = object(input.args);
	const details = object(input.result?.details);
	const lines: string[] = [];
	if (args.network === true)
		lines.push("Requested: outbound network for tools");
	else if (typeof args.path === "string") {
		const scope =
			details.scope === "file"
				? "file"
				: "path (directories include the entire tree)";
		lines.push(
			`Requested: ${args.access === "write" ? "write" : "read"} ${scope}: ${args.path}`,
		);
	} else lines.push("Scope incomplete; waiting for request arguments");
	if (typeof args.reason === "string") lines.push(`Reason: ${args.reason}`);
	let title: string;
	if (input.phase !== "complete")
		title =
			input.phase === "pending"
				? "Access request · Checking policy and approval"
				: "Access request · preparing";
	else if (input.result?.isError) {
		// Only authority-owned, versioned details carry attribution. Never parse content/args.
		const feedback = object(details.permissionFeedback);
		const view =
			feedback.schema === "oq.permission-feedback.v1"
				? permissionFeedback(
						{ feedback },
						feedback.execution === "uncertain" ? "uncertain" : "not_started",
					)
				: permissionFeedback(undefined);
		title = view.title;
		lines.push(view.message, `Next: ${view.guidance}`);
		if (["unknown", "expired", "unavailable"].includes(view.code)) {
			lines.push(
				"This outcome does not confirm new authorization; review current grants with /permissions.",
			);
		}
		lines.push(
			view.execution === "uncertain"
				? "Effects uncertain; inspect before retrying."
				: "The confined operation was not started.",
		);
	} else {
		const granted = object(details.granted);
		if (
			typeof granted.capability === "string" &&
			typeof granted.resource === "string"
		) {
			title = "Recorded grant outcome";
			lines.push(`Recorded: ${granted.capability} ${granted.resource}`);
			if (
				typeof details.expiresAt === "number" &&
				Number.isFinite(details.expiresAt)
			) {
				const expiry = new Date(details.expiresAt);
				if (Number.isFinite(expiry.getTime()))
					lines.push(`Recorded expiry: ${expiry.toISOString()}`);
			}
			lines.push(
				"/permissions shows current access. Historical or imported results do not restore a grant.",
			);
			lines.push(
				args.network === true
					? "Network access is for tools. Each tool call still asks separately."
					: "Directory access covers its tree. Each tool call still asks separately.",
			);
		} else {
			title = "Access request completed";
			lines.push(
				"Inspect raw details for the actual scope; no grant inferred from text.",
			);
		}
	}
	lines.push("/permission-help for safe next steps");
	return {
		title: permissionDisplayText(title),
		lines: lines.map(permissionDisplayText),
	};
}
