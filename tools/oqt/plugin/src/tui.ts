import { type Context, defineFacet } from "@earendil-works/chord";
import { BACKGROUND_CONTEXT } from "@earendil-works/chord/context";
import {
	PresentationUI,
	SlashCommands,
} from "@earendil-works/pi-coding-agent/experimental/plugin";
import { type PendingPermission, Permissions } from "./contract.ts";
import { guardPermissions } from "./guard-client.ts";

export default defineFacet({
	id: "oq.permissions.demo.tui",
	setup(env) {
		// Coding mode: decisions go straight to the guard's operator socket, which
		// no brain can reach. The synthetic demo keeps its worker-provided service.
		const operatorPath = process.env.OQT_GUARD_OPERATOR;
		const guard = operatorPath ? guardPermissions(operatorPath) : undefined;
		if (guard) env.own(() => guard.close());
		const demo = guard ? undefined : env.use(Permissions);
		const permissions = guard ?? (demo as NonNullable<typeof demo>);
		const coding = guard !== undefined;
		let reportedInterrupted = 0;
		const commands = env.use(SlashCommands);
		const ui = env.use(PresentationUI);
		const shown = new Set<string>();
		let selecting = false;
		const choose = async (request: PendingPermission, context: Context) => {
			if (selecting) return;
			selecting = true;
			shown.add(request.digest);
			try {
				const action = await ui.select(
					`${coding ? "Coding" : "DEMO"} permission · ${request.capability} / ${request.resource}\nArguments: ${request.argumentsText}\nSession: ${request.session}`,
					[
						{ value: "once", label: "Allow once" },
						{
							value: "session",
							label: coding
								? "Allow workspace tools for this Session (60 seconds)"
								: "Allow for this Session (60 seconds)",
						},
						{ value: "deny", label: "Deny" },
					],
					"deny",
					context,
				);
				if (action === undefined) {
					ui.showStatus(
						"Permission remains pending. Use /permissions to decide.",
						context,
					);
					return;
				}
				const decision = {
					id: request.id,
					digest: request.digest,
					revision: request.revision,
					allow: action !== "deny",
					lifetime:
						action === "session" ? ("session" as const) : ("once" as const),
				};
				if (guard)
					await guard.decide(
						{ ...decision, session: request.session },
						context,
					);
				else await demo?.decide(decision, context);
				ui.showStatus(
					action === "deny"
						? "Permission denied"
						: "Permission applied; continuing the same turn",
					context,
				);
			} catch (error) {
				ui.showStatus(`Permission decision failed: ${String(error)}`, context);
			} finally {
				selecting = false;
			}
		};
		env.onActivate(() => {
			const bannerTimer = setTimeout(
				() =>
					ui.showStatus(
						coding
							? "oqt coding · owning Pi model · confined brain and workspace tools · exact approvals · /permissions revoke"
							: "oqt DEMO · synthetic model/history · confined fixture files · no credentials/local tools. Try: own history, global history, read file",
						BACKGROUND_CONTEXT,
					),
				1000,
			);
			env.own(() => clearTimeout(bannerTimer));
			ui.showStatus(
				coding
					? "oqt coding · owning Pi model · confined brain and workspace tools · exact approvals"
					: "oqt DEMO · synthetic model/history · real permission gates and confined fixture hands. Try: own history, global history, read file",
				BACKGROUND_CONTEXT,
			);
			env.own(
				permissions.state.subscribe((state, context) => {
					const interrupted =
						"interrupted" in state && Array.isArray(state.interrupted)
							? state.interrupted
							: [];
					if (interrupted.length > reportedInterrupted) {
						const last = interrupted[interrupted.length - 1];
						ui.showStatus(
							`Interrupted: ${last.tool} ${last.argumentsText.slice(0, 120)} — effects uncertain; grants revoked. Inspect, then /permissions ack`,
							context,
						);
					}
					reportedInterrupted = interrupted.length;
					const request = state.pending.find((row) => !shown.has(row.digest));
					if (request) void choose(request, context);
				}),
			);
			env.own(
				commands.replace({
					name: "permissions",
					description:
						"Inspect/decide permissions; /permissions revoke stops hands and clears approved overlays",
					argumentHint: "[revoke|ack]",
					async run(args, context) {
						if (args.trim() === "ack") {
							if (!guard) {
								ui.showStatus("Nothing to acknowledge in the demo", context);
								return undefined;
							}
							await guard.acknowledge(context);
							ui.showStatus(
								"Interrupted calls acknowledged as inspected; nothing was replayed",
								context,
							);
							return undefined;
						}
						if (args.trim() === "revoke") {
							await permissions.revokeAll(context);
							ui.showStatus(
								"Approved overlays revoked; previously disclosed data cannot be erased",
								context,
							);
							return undefined;
						}
						const request = permissions.state.value?.pending[0];
						if (request) await choose(request, context);
						else ui.showStatus("No pending permission requests", context);
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
