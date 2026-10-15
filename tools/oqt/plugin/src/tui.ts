import { type Context, defineFacet } from "@earendil-works/chord";
import { BACKGROUND_CONTEXT } from "@earendil-works/chord/context";
import {
	PresentationUI,
	SlashCommands,
} from "@earendil-works/pi-coding-agent/experimental/plugin";
import { Models } from "../../../../src/experimental/services/models.ts";
import {
	type PendingPermission,
	type PermissionDecision,
	Permissions,
} from "./contract.ts";
import { guardPermissions, type OperatorState } from "./guard-client.ts";
import { installModelCommands } from "./models.ts";

type Choice = "once" | "session" | "always" | "deny";

const HEADERS: Record<string, string> = {
	shell: "Bash command",
	edit: "File change",
	read: "Read file",
	resource: "Access outside the workspace",
};

/** Menu items for one pending request: what "session"/"always" would cover. */
function menu(request: PendingPermission, coding: boolean) {
	if (!coding)
		return [
			{ value: "once", label: "1. Allow once" },
			{ value: "session", label: "2. Allow for this Session (60 seconds)" },
			{ value: "deny", label: "3. Deny" },
		];
	if (request.category === "resource")
		return [
			{ value: "session", label: "1. Yes, for this session (1 hour)" },
			{ value: "deny", label: "2. No" },
		];
	const scope =
		request.category === "shell"
			? request.prefix
				? `\`${request.prefix}\` commands`
				: "all shell commands"
			: request.category === "edit"
				? "file changes"
				: "reads";
	return [
		{ value: "once", label: "1. Yes" },
		{
			value: "session",
			label: `2. Yes, and don't ask again for ${scope} this session`,
		},
		{
			value: "always",
			label: `3. Yes, and always allow ${scope} in this repo`,
		},
		{ value: "deny", label: "4. No" },
	];
}

function title(request: PendingPermission, coding: boolean) {
	if (!coding)
		return `DEMO permission · ${request.capability} / ${request.resource}\nArguments: ${request.argumentsText}\nSession: ${request.session}`;
	const header = HEADERS[request.category ?? ""] ?? "Permission";
	if (request.category === "resource")
		return `${header}\n\n  ${request.capability} ${request.resource}\n  Reason: ${request.argumentsText}\n\nMounted on fresh tools; each call still asks.  Enter/1-2 · Esc = No`;
	const preview = (request.preview ?? request.argumentsText)
		.split("\n")
		.map((line) => `  ${line}`)
		.join("\n");
	return `${header}\n\n${preview}\n\nDo you want to proceed?  Enter/1-4 · Esc = No`;
}

export default defineFacet({
	id: "oq.permissions.demo.tui",
	setup(env) {
		// Coding mode: decisions go straight to the guard's operator socket, which
		// no brain can reach. The synthetic demo keeps its worker-provided service.
		const operatorPath = process.env.OQT_GUARD_OPERATOR;
		const guard = operatorPath ? guardPermissions(operatorPath) : undefined;
		if (guard) env.own(() => guard.close());
		const demo = guard ? undefined : env.use(Permissions);
		const coding = guard !== undefined;
		const state = (): OperatorState | undefined =>
			guard
				? guard.state.value
				: (demo?.state.value as OperatorState | undefined);
		const commands = env.use(SlashCommands);
		const ui = env.use(PresentationUI);
		const models = env.use(Models);
		const shown = new Set<string>();
		let reportedInterrupted = 0;
		let selecting = false;
		const decide = async (
			decision: PermissionDecision,
			session: string,
			context: Context,
		) => {
			if (guard) await guard.decide({ ...decision, session }, context);
			else await demo?.decide(decision, context);
		};
		const choose = async (request: PendingPermission, context: Context) => {
			if (selecting) return;
			selecting = true;
			shown.add(request.digest);
			try {
				const items = menu(request, coding);
				const picked = (await ui.select(
					title(request, coding),
					items,
					coding ? items[0].value : "deny",
					context,
				)) as Choice | undefined;
				// Esc means No in coding mode; the demo keeps "decide later".
				if (picked === undefined && !coding) {
					ui.showStatus(
						"Permission remains pending. Use /permissions to decide.",
						context,
					);
					return;
				}
				const choice: Choice = picked ?? "deny";
				await decide(
					{
						id: request.id,
						digest: request.digest,
						revision: request.revision,
						allow: choice !== "deny",
						lifetime:
							choice === "once" || choice === "deny" ? "once" : "session",
						...(choice === "always" ? { remember: true } : {}),
					},
					request.session,
					context,
				);
				const scope = request.prefix
					? `\`${request.prefix}\``
					: request.category;
				if (!coding) {
					ui.showStatus(
						choice === "deny"
							? "Permission denied"
							: "Permission applied; continuing the same turn",
						context,
					);
					return;
				}
				ui.showStatus(
					choice === "deny"
						? "Denied; the agent was told"
						: choice === "always"
							? `Always allowing ${scope} in this repo · /permissions to manage`
							: choice === "session"
								? `Allowing ${scope} for this session · /permissions to manage`
								: "Approved; continuing the same turn",
					context,
				);
			} catch (error) {
				ui.showStatus(`Permission decision failed: ${String(error)}`, context);
			} finally {
				selecting = false;
				// Another request may have queued while this one was open.
				const next = state()?.pending.find((row) => !shown.has(row.digest));
				if (next) void choose(next, context);
			}
		};
		const describeRules = () => {
			const value = state();
			const rules = value?.rules;
			const lines: string[] = [];
			if (rules) {
				const kinds = [
					rules.read ? "reads" : "",
					rules.edit ? "file changes" : "",
					rules.shell ? "all shell commands" : "",
					...rules.shell_prefixes.map((prefix) => `\`${prefix}\``),
				].filter(Boolean);
				lines.push(
					`Always allowed here: ${kinds.length ? kinds.join(", ") : "nothing"}`,
				);
			}
			for (const resource of value?.resources ?? [])
				lines.push(
					`Session access: ${resource.capability} ${resource.resource} until ${new Date(resource.expiresAt).toLocaleTimeString()}`,
				);
			return lines.join(" · ") || "No remembered rules or active access grants";
		};
		const manage = async (context: Context) => {
			const value = state();
			const pending = value?.pending ?? [];
			const items = [
				...pending.map((request, index) => ({
					value: `pending:${index}`,
					label: `Decide: ${HEADERS[request.category ?? ""] ?? request.capability} — ${(request.preview ?? request.argumentsText).split("\n")[0].slice(0, 80)}`,
				})),
				{ value: "rules", label: "Show remembered rules and active access" },
				{
					value: "revoke",
					label: "Revoke this session's grants (stops tools)",
				},
				...(coding
					? [
							{
								value: "forget",
								label: 'Forget all "always" rules for this repo',
							},
						]
					: []),
				...((value?.interrupted.length ?? 0) > 0
					? [
							{
								value: "ack",
								label: "Acknowledge interrupted calls (inspected)",
							},
						]
					: []),
			];
			const picked = await ui.select(
				"Permissions",
				items,
				items[0]?.value,
				context,
			);
			if (picked === undefined) return;
			if (picked.startsWith("pending:")) {
				const request = pending[Number(picked.slice(8))];
				if (request) {
					shown.delete(request.digest);
					await choose(request, context);
				}
			} else if (picked === "rules") ui.showStatus(describeRules(), context);
			else if (picked === "revoke") await revoke(context);
			else if (picked === "forget") {
				await guard?.forgetRules(context);
				ui.showStatus(
					"Forgot this repo's rules and revoked their grants",
					context,
				);
			} else if (picked === "ack") await acknowledge(context);
		};
		const revoke = async (context: Context) => {
			if (guard) await guard.revokeAll(context);
			else await demo?.revokeAll(context);
			ui.showStatus(
				"Session grants revoked; previously disclosed data cannot be erased",
				context,
			);
		};
		const acknowledge = async (context: Context) => {
			if (!guard) {
				ui.showStatus("Nothing to acknowledge in the demo", context);
				return;
			}
			await guard.acknowledge(context);
			ui.showStatus(
				"Interrupted calls acknowledged as inspected; nothing was replayed",
				context,
			);
		};
		env.onActivate(() => {
			installModelCommands({
				commands,
				ui,
				models,
				own: (dispose) => env.own(dispose),
			});
			const banner = coding
				? "oqt coding · confined brain and tools · reads allowed, edits and shell ask · /permissions"
				: "oqt DEMO · synthetic model/history · confined fixture files · no credentials/local tools. Try: own history, global history, read file";
			const bannerTimer = setTimeout(
				() => ui.showStatus(banner, BACKGROUND_CONTEXT),
				1000,
			);
			env.own(() => clearTimeout(bannerTimer));
			ui.showStatus(banner, BACKGROUND_CONTEXT);
			const listen = (value: OperatorState, context: Context) => {
				const interrupted = value.interrupted ?? [];
				if (interrupted.length > reportedInterrupted) {
					const last = interrupted[interrupted.length - 1];
					ui.showStatus(
						`Interrupted: ${last.tool} ${last.argumentsText.slice(0, 120)} — effects uncertain; grants revoked. Inspect, then /permissions ack`,
						context,
					);
				}
				reportedInterrupted = interrupted.length;
				const request = value.pending.find((row) => !shown.has(row.digest));
				if (request) void choose(request, context);
			};
			env.own(
				guard
					? guard.state.subscribe(listen)
					: (demo as NonNullable<typeof demo>).state.subscribe(
							(value, context) =>
								listen(
									{
										...(value as unknown as OperatorState),
										interrupted: [],
										resources: [],
									},
									context,
								),
						),
			);
			env.own(
				commands.replace({
					name: "permissions",
					description:
						"Manage permissions: pending requests, rules, revoke (revoke|ack|rules|forget)",
					argumentHint: "[revoke|ack|rules|forget]",
					async run(args, context) {
						const sub = args.trim();
						if (sub === "revoke") await revoke(context);
						else if (sub === "ack") await acknowledge(context);
						else if (sub === "rules") ui.showStatus(describeRules(), context);
						else if (sub === "forget" && guard) {
							await guard.forgetRules(context);
							ui.showStatus(
								"Forgot this repo's rules and revoked their grants",
								context,
							);
						} else await manage(context);
						return undefined;
					},
				}),
			);
			env.own(
				commands.replace({
					name: "reload",
					description: "Runtime code reload is disabled for the pinned runtime",
					async run(_args, context) {
						ui.showStatus(
							"This Session pins its runtime. Permission changes apply live; plugin code reload is disabled.",
							context,
						);
						return undefined;
					},
				}),
			);
		});
	},
});
