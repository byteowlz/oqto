import { type Context, defineFacet } from "@earendil-works/chord";
import { BACKGROUND_CONTEXT } from "@earendil-works/chord/context";
import {
	PresentationUI,
	SlashCommands,
} from "@earendil-works/pi-coding-agent/experimental/plugin";
import { type PendingPermission, Permissions } from "./contract.ts";

export default defineFacet({
	id: "oq.permissions.demo.tui",
	setup(env) {
		const permissions = env.use(Permissions);
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
					`DEMO permission · ${request.capability} / ${request.resource}\nArguments: ${request.argumentsText}\nSession: ${request.session}`,
					[
						{ value: "once", label: "Allow once" },
						{ value: "session", label: "Allow for this Session (60 seconds)" },
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
				await permissions.decide(
					{
						id: request.id,
						digest: request.digest,
						revision: request.revision,
						allow: action !== "deny",
						lifetime: action === "session" ? "session" : "once",
					},
					context,
				);
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
						"oqt DEMO · synthetic model/history · confined fixture files · no credentials/local tools. Try: own history, global history, read file",
						BACKGROUND_CONTEXT,
					),
				1000,
			);
			env.own(() => clearTimeout(bannerTimer));
			ui.showStatus(
				"oqt DEMO · synthetic model/history · real permission gates and confined fixture hands. Try: own history, global history, read file",
				BACKGROUND_CONTEXT,
			);
			env.own(
				permissions.state.subscribe((state, context) => {
					const request = state.pending.find((row) => !shown.has(row.digest));
					if (request) void choose(request, context);
				}),
			);
			env.own(
				commands.replace({
					name: "permissions",
					description:
						"Inspect/decide demo permissions; /permissions revoke clears approved overlays",
					argumentHint: "[revoke]",
					async run(args, context) {
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
					description: "Runtime code reload is disabled for the pinned demo",
					async run(_args, context) {
						ui.showStatus(
							"This demo pins its runtime. Permission changes apply live; plugin code reload is disabled.",
							context,
						);
						return undefined;
					},
				}),
			);
		});
	},
});
