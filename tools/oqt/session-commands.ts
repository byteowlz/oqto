// Copied beside the upstream client TUI. Classic-pi session commands for the
// oqt client: /new and /resume (searchable). The client adopts the target
// Session first, so worker-loss reattachment never pulls it back.
import { type Context, defineFacet } from "@earendil-works/chord";
import { PresentationUI } from "./services/presentation-ui.ts";
import {
	SessionDirectory,
	SessionManagement,
	type SessionSummary,
} from "./services/sessions.ts";
import { SlashCommands } from "./services/slash-commands.ts";

export interface SessionSwitcher {
	currentSessionId(): string | undefined;
	switchTo(sessionId: string, management: SessionManagement): Promise<void>;
}

function when(timestamp: number) {
	const date = new Date(timestamp);
	const today = new Date();
	const time = date.toLocaleTimeString([], {
		hour: "2-digit",
		minute: "2-digit",
	});
	return date.toDateString() === today.toDateString()
		? `today ${time}`
		: `${date.toLocaleDateString()} ${time}`;
}

export function createOqtSessionFacet(switcher: SessionSwitcher) {
	return defineFacet({
		id: "oqt.session-commands",
		setup(env) {
			const commands = env.use(SlashCommands);
			const ui = env.use(PresentationUI);
			const management = env.use(SessionManagement);
			const directory = env.use(SessionDirectory);
			const sessions = (): SessionSummary[] =>
				[...(directory.state.value?.sessions ?? [])].sort(
					(left, right) => right.createdAt - left.createdAt,
				);
			const switchTo = async (sessionId: string, context: Context) => {
				ui.showStatus("Switching session…", context);
				await switcher.switchTo(sessionId, management);
				ui.showStatus(`Session ${sessionId.slice(0, 8)}`, context);
			};
			env.onActivate(() => {
				env.own(
					commands.replace({
						name: "new",
						description: "Start a new session in this workspace",
						async run(_args, context) {
							const created = await management.create({}, context);
							await switchTo(created.sessionId, context);
							return undefined;
						},
					}),
				);
				env.own(
					commands.replace({
						name: "resume",
						description: "Resume a session in this workspace (type to search)",
						async run(args, context) {
							const all = sessions();
							const current = switcher.currentSessionId();
							const wanted = args.trim();
							if (wanted) {
								const match = all.find((session) =>
									session.sessionId.startsWith(wanted),
								);
								if (!match)
									throw new Error(`No session starting with ${wanted}`);
								await switchTo(match.sessionId, context);
								return undefined;
							}
							if (all.length === 0) {
								ui.showStatus("No sessions in this workspace yet", context);
								return undefined;
							}
							const picked = await ui.select(
								"Resume session",
								all.map((session) => ({
									value: session.sessionId,
									label: `${when(session.createdAt)}${session.sessionId === current ? "  (current)" : ""}`,
									description: session.sessionId,
								})),
								current,
								context,
							);
							if (picked !== undefined && picked !== current)
								await switchTo(picked, context);
							return undefined;
						},
					}),
				);
			});
		},
	});
}
