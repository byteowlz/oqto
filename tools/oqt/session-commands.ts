// Copied beside the upstream client TUI. Classic-pi session commands for the
// oqt client: /new and /resume (searchable). The client adopts the target
// Session first, so worker-loss reattachment never pulls it back.
import { join } from "node:path";
import { type Context, defineFacet } from "@earendil-works/chord";
import type {
	ConversationId,
	Cursor,
	EntryId,
} from "@earendil-works/pi-durable";
import { PresentationUI } from "./services/presentation-ui.ts";
import {
	SessionDirectory,
	SessionManagement,
	type SessionSummary,
} from "./services/sessions.ts";
import { SlashCommands } from "./services/slash-commands.ts";
import { SessionForks } from "./session-forks.ts";

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
			const forks = env.use(SessionForks);
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
				// Bounded pages with opaque native cursors; labels never supply identity.
				env.own(
					commands.replace({
						name: "tree",
						description: "Select a Durable conversation in this session",
						async run(_args, context) {
							const selected = await forks.selected(context);
							let cursor: Cursor | undefined;
							do {
								const page = await forks.conversations(
									50,
									cursor ?? null,
									context,
								);
								const picked = await ui.select(
									"Conversations",
									[
										...page.items.map((row) => ({
											value: String(row.id),
											label: `Conversation ${row.id}${row.id === selected ? " (current)" : ""}`,
											description: row.parent
												? `Fork of ${row.parent.conversationId} at entry ${row.parent.at}`
												: "Root / independent",
										})),
										...(page.next
											? [{ value: "more", label: "Next page" }]
											: []),
									],
									String(selected),
									context,
								);
								if (picked === "more") {
									cursor = page.next;
									continue;
								}
								if (picked !== undefined)
									await forks.select(Number(picked) as ConversationId, context);
								return undefined;
							} while (cursor);
							return undefined;
						},
					}),
				);
				env.own(
					commands.replace({
						name: "branch",
						description:
							"Fork the selected conversation at an explicit native entry",
						async run(args, context) {
							const selected = await forks.selected(context);
							let at: number | undefined;
							if (args.trim()) {
								if (!/^[1-9][0-9]*$/.test(args.trim()))
									throw new Error("Expected a native entry ID");
								at = Number(args.trim());
							} else {
								let cursor: Cursor | undefined;
								do {
									const page = await forks.entries(
										selected,
										50,
										cursor ?? null,
										context,
									);
									const picked = await ui.select(
										"Branch at entry (inclusive)",
										[
											...page.items.map((row) => ({
												value: String(row.id),
												label: `Entry ${row.id}: ${row.kind}`,
												description: `Conversation ${row.conversationId}`,
											})),
											...(page.next
												? [{ value: "more", label: "Next page" }]
												: []),
										],
										undefined,
										context,
									);
									if (picked === "more") {
										cursor = page.next;
										continue;
									}
									if (picked === undefined) return undefined;
									at = Number(picked);
									break;
								} while (cursor);
							}
							if (at !== undefined) {
								const id = await forks.fork(selected, at as EntryId, context);
								ui.showStatus(`Conversation ${id}`, context);
							}
							return undefined;
						},
					}),
				);
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
							const classic = args.trim().startsWith("--")
								? (await import("./classic-client.ts")).parseClassicResumeArgs(
										args,
									)
								: undefined;
							if (classic) {
								const operatorPath = process.env.OQT_GUARD_OPERATOR;
								if (!operatorPath)
									throw new Error(
										"Classic import requires the local operator endpoint",
									);
								let path = classic.path;
								let expectedHash: string | undefined;
								if (path === undefined) {
									const owner = process.env.OQT_OWNER_AGENT_DIR;
									if (!owner)
										throw new Error(
											"Classic discovery requires OQT_OWNER_AGENT_DIR",
										);
									const { discoverClassicSources } = await import(
										"./classic-source.ts"
									);
									const discovered = await discoverClassicSources(
										join(owner, "sessions"),
										classic.origin,
									);
									if (discovered.skipped.length > 0 || discovered.truncated)
										ui.showStatus(
											`Classic discovery: ${discovered.skipped.length} skipped${discovered.truncated ? "; results truncated" : ""}`,
											context,
										);
									if (!discovered.sources.length) {
										ui.showStatus("No classic sources found", context);
										return undefined;
									}
									path = await ui.select(
										"Import classic source",
										discovered.sources.map((source) => ({
											value: source.path,
											label: JSON.stringify(source.sourceId),
											description: `${JSON.stringify(source.path)} · cwd ${JSON.stringify(source.cwd)} · ${source.leaves.length} leaves`,
										})),
										undefined,
										context,
									);
									if (path === undefined) return undefined;
									const selected = discovered.sources.find(
										(source) => source.path === path,
									);
									if (!selected)
										throw new Error("Unknown selected classic source");
									expectedHash = selected.sourceHash;
								}
								context.abortSignal?.throwIfAborted();
								const { importClassicSource } = await import(
									"./classic-client.ts"
								);
								const receipt = await importClassicSource(
									operatorPath,
									path,
									classic.origin,
									{
										...(classic.leafId !== undefined
											? { leafId: classic.leafId }
											: {}),
										...(context.abortSignal
											? { signal: context.abortSignal }
											: {}),
										...(expectedHash !== undefined ? { expectedHash } : {}),
									},
								);
								await switchTo(receipt.sessionId, context);
								return undefined;
							}
							const all = sessions();
							const current = switcher.currentSessionId();
							const wanted = args.trim();
							if (wanted) {
								const matches = all.filter((session) =>
									session.sessionId.startsWith(wanted),
								);
								if (matches.length > 1)
									throw new Error(`Ambiguous session prefix: ${wanted}`);
								const match = matches[0];
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
