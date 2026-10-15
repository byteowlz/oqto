"""Exact, reviewed edits to the private upstream derivative.

Each `old` must occur exactly once in the pinned source or preparation fails closed.
Keep this list small; every entry is a candidate upstream fix.
"""

PATCHES = [
    {
        "id": "client-tui-reattach-after-worker-loss",
        "why": "Over Unix transport the TUI ignored attachment loss: after a session worker "
               "crash it spun forever on the dead turn. Reattach the same Session so the server "
               "starts a fresh worker and native Durable recovery settles the interrupted task.",
        "file": "packages/coding-agent/src/experimental/client-tui.ts",
        "old": """				const commands = env.use(SlashCommands);
				const controller = env.use(AgentController);""",
        "new": """				const commands = env.use(SlashCommands);
				const controller = env.use(AgentController);
				const management = env.use(SessionManagement);""",
    },
    {
        "id": "client-tui-subscribe-attachment-all-transports",
        "why": "Attachment state must drive recovery for Unix as well as Radius transports.",
        "file": "packages/coding-agent/src/experimental/client-tui.ts",
        "old": """					if (server.radius) {
						env.own(
							server.server.connection.subscribe((state) => this.#handleConnectionState(server.serverId, state)),
						);
						env.own(
							server.session.attachment.subscribe((state) => this.#handleAttachmentState(sessionFeature, state)),
						);
					}""",
        "new": """					if (server.radius) {
						env.own(
							server.server.connection.subscribe((state) => this.#handleConnectionState(server.serverId, state)),
						);
					}
					env.own(
						server.session.attachment.subscribe((state) => {
							if (state.status === "detached" && !server.radius)
								this.#reattachAfterWorkerLoss(sessionFeature, management);
							this.#handleAttachmentState(sessionFeature, state);
						}),
					);""",
    },
    {
        "id": "client-tui-reattach-method",
        "why": "Single bounded reattach path; a closed client or another Session never reattaches.",
        "file": "packages/coding-agent/src/experimental/client-tui.ts",
        "old": """	#queueRecovery(operation: () => Promise<void>): void {""",
        "new": """	#reattachAfterWorkerLoss(feature: SessionFeature, management: SessionManagement): void {
		if (this.#closed || this.#selectedServerId !== feature.serverId || this.#sessionId === undefined) return;
		const sessionId = this.#sessionId;
		this.#busy = true;
		this.#status = "Session worker lost; recovering the Session…";
		this.#rebuild();
		this.#queueRecovery(async () => {
			await this.#closeLane();
			if (this.#sessionId !== sessionId) return;
			await management.attach(sessionId, BACKGROUND_CONTEXT);
		});
	}

	#queueRecovery(operation: () => Promise<void>): void {""",
    },
    {
        "id": "session-worker-manager-confined-spawn",
        "why": "Coding sessions run their worker as a guard-launched confined brain; the guard "
               "decides, the brain cannot reach owner credentials or the permission ledger.",
        "file": "packages/coding-agent/src/experimental/session-worker-manager.ts",
        "old": """			child = spawnInternalProcess("session-worker", [JSON.stringify(options)], {""",
        "new": """			child = spawnOqtSessionWorker(options, {""",
    },
    {
        "id": "session-worker-manager-confined-spawn-import",
        "why": "Import the guard spawn hook beside upstream's spawn helper.",
        "file": "packages/coding-agent/src/experimental/session-worker-manager.ts",
        "old": """import { spawnInternalProcess } from "./process.ts";""",
        "new": """import { spawnOqtSessionWorker } from "./spawn-hook.ts";""",
    },
    {
        "id": "server-report-errors-to-host-log",
        "why": "Upstream's experimental server wires no onError, so internal failures reach "
               "clients only as 'Internal server error'. Keep the cause in the private host log.",
        "file": "packages/coding-agent/src/experimental/server.ts",
        "old": """		mode: 0o600,
		onConnectionCountChanged,
	});""",
        "new": """		mode: 0o600,
		onConnectionCountChanged,
		onError: (error) => console.error("oqt server:", error),
	});""",
    },
    {
        "id": "session-worker-report-host-pid-reconnect",
        "why": "A confined brain runs in its own PID namespace. The manager binds a registering "
               "worker to the pid it spawned (the brain's bwrap) and later signals that pid, so the "
               "brain reports the guard-attested host pid. The manager's equality check is unchanged.",
        "file": "packages/coding-agent/src/experimental/session-worker.ts",
        "old": """				sessionId,
				pid: process.pid,
				metadata,
				pluginManifestPaths: [...pluginManifestPaths],
			})
			.catch(() => closeAndExit());""",
        "new": """				sessionId,
				pid: Number(process.env.OQT_HOST_PID ?? process.pid),
				metadata,
				pluginManifestPaths: [...pluginManifestPaths],
			})
			.catch(() => closeAndExit());""",
    },
    {
        "id": "session-worker-report-host-pid-initial",
        "why": "Same as session-worker-report-host-pid-reconnect, for the first readiness report.",
        "file": "packages/coding-agent/src/experimental/session-worker.ts",
        "old": """		ready = true;
		await control.send({
			type: "worker_ready",
			token,
			sessionKey,
			sessionId,
			pid: process.pid,""",
        "new": """		ready = true;
		await control.send({
			type: "worker_ready",
			token,
			sessionKey,
			sessionId,
			pid: Number(process.env.OQT_HOST_PID ?? process.pid),""",
    },
    {
        "id": "client-tui-classic-chrome",
        "why": "Render like classic Pi: no Server/Session heading; footer shows cwd, model, "
               "thinking and cumulative usage instead of an entry counter.",
        "file": "packages/coding-agent/src/experimental/client-tui.ts",
        "old": """		this.#sessionHeading.setText(
			this.#sessionId === undefined || this.#selectedServerId === undefined
				? ""
				: theme.fg("dim", `Server: ${this.#selectedServerId}\\nSession: ${this.#sessionId}`),
		);""",
        "new": """		// Classic Pi shows no server/session heading (see /sessions instead).
		this.#sessionHeading.setText("");""",
    },
    {
        "id": "client-tui-classic-footer",
        "why": "Classic-style footer: cwd, model, thinking and cumulative token usage.",
        "file": "packages/coding-agent/src/experimental/client-tui.ts",
        "old": """	#footer(): string {
		const view = this.#conversationView();
		if (!view) return "/model · /thinking · /compact · /reload";
		const agent = (view.docs["pi.agent"] ?? {}) as AgentState;
		const model = agent.model === undefined ? "no model" : `${agent.model.provider}/${agent.model.modelId}`;
		return `${model} · thinking:${agent.thinkingLevel ?? "off"} · ${view.entries.length} entries · /model · /thinking · /compact · /reload`;
	}""",
        "new": """	#footer(): string {
		const view = this.#conversationView();
		if (!view) return "/model · /thinking · /compact · /reload";
		const agent = (view.docs["pi.agent"] ?? {}) as AgentState;
		const model = agent.model === undefined ? "no model" : `${agent.model.provider}/${agent.model.modelId}`;
		// Classic-style: cwd, model, thinking, cumulative token usage.
		let inputTokens = 0;
		let outputTokens = 0;
		for (const entry of view.entries)
			for (const message of entry.model ?? [])
				if (message.role === "assistant" && message.usage) {
					inputTokens += message.usage.input ?? 0;
					outputTokens += message.usage.output ?? 0;
				}
		return `${process.cwd()} · ${model} · thinking:${agent.thinkingLevel ?? "off"} · ↑${formatTokens(inputTokens)} ↓${formatTokens(outputTokens)} · /model · /thinking · /compact`;
	}""",
    },
    {
        "id": "client-tui-classic-footer-import",
        "why": "Import the shared token formatter for the classic-style footer.",
        "file": "packages/coding-agent/src/experimental/client-tui.ts",
        "old": """import { createInteractiveTui } from "../modes/interactive/tui-renderer.ts";""",
        "new": """import { formatTokens } from "../modes/interactive/components/footer.ts";
import { createInteractiveTui } from "../modes/interactive/tui-renderer.ts";""",
    },
    {
        "id": "client-tui-stable-selector",
        "why": "The selector was rebuilt on every redraw (any transcript/status update) and "
               "reset to its default item, so arrow keys and Ctrl+C appeared dead in dialogs. "
               "Build it once per selection and keep it across redraws.",
        "file": "packages/coding-agent/src/experimental/client-tui.ts",
        "old": """		if (this.#screen === "select" && this.#selection !== undefined) {
			this.#chatInput.focused = false;
			const selector = new Container();
			selector.addChild(new Text(theme.bold(this.#selection.title), 1, 1));
			const items = [...this.#selection.items];
			this.#selectList = new SelectList(items, Math.min(Math.max(items.length, 1), 12), selectTheme);
			const selectedIndex = items.findIndex((item) => item.value === this.#selection?.selectedValue);
			if (selectedIndex >= 0) this.#selectList.setSelectedIndex(selectedIndex);
			this.#selectList.onSelect = (item) => this.#completeSelection(item.value);
			this.#selectList.onCancel = () => this.#completeSelection(undefined);
			selector.addChild(this.#selectList);
			this.#editorContainer.addChild(selector);
		} else {""",
        "new": """		if (this.#screen === "select" && this.#selection !== undefined) {
			this.#chatInput.focused = false;
			if (
				this.#selectorFor !== this.#selection ||
				this.#selectorQuery !== this.#selectQuery ||
				this.#selector === undefined
			) {
				const selector = new Container();
				const all = [...this.#selection.items];
				const searchable = all.length > 9 || this.#selectQuery !== "";
				const query = this.#selectQuery;
				const items = query
					? fuzzyFilter(all, query, (item) => `${item.label} ${item.description ?? ""} ${item.value}`)
					: all;
				const hint = searchable
					? theme.fg("dim", query ? `  search: ${query}  (${items.length}/${all.length})` : "  type to search")
					: "";
				selector.addChild(new Text(`${theme.bold(this.#selection.title)}${hint ? `\n${hint}` : ""}`, 1, 1));
				this.#selectList = new SelectList(items, Math.min(Math.max(items.length, 1), 12), selectTheme);
				const selectedIndex = items.findIndex((item) => item.value === this.#selection?.selectedValue);
				if (selectedIndex >= 0) this.#selectList.setSelectedIndex(selectedIndex);
				this.#selectList.onSelect = (item) => this.#completeSelection(item.value);
				this.#selectList.onCancel = () => this.#completeSelection(undefined);
				selector.addChild(this.#selectList);
				this.#selector = selector;
				this.#selectorFor = this.#selection;
				this.#selectorQuery = this.#selectQuery;
			}
			this.#editorContainer.addChild(this.#selector);
		} else {
			this.#selector = undefined;
			this.#selectorFor = undefined;
			this.#selectQuery = "";""",
    },
    {
        "id": "client-tui-stable-selector-fields",
        "why": "State for the stable selector.",
        "file": "packages/coding-agent/src/experimental/client-tui.ts",
        "old": """	readonly #sessionHeading = new Text("", 1, 0);""",
        "new": """	readonly #sessionHeading = new Text("", 1, 0);
	#selector: Container | undefined;
	#selectorFor: unknown;
	#selectorQuery = "";
	#selectQuery = "";""",
    },
    {
        "id": "client-tui-select-number-keys",
        "why": "Number keys pick a selector item directly (1-9).",
        "file": "packages/coding-agent/src/experimental/client-tui.ts",
        "old": """		this.#selectList?.handleInput(data);
	}""",
        "new": """		const selection = this.#selection;
		if (selection !== undefined && selection.items.length <= 9 && !this.#selectQuery) {
			const digit = /^[1-9]$/.test(data) ? Number(data) - 1 : -1;
			const item = digit >= 0 ? selection.items[digit] : undefined;
			if (item !== undefined) {
				this.#completeSelection(item.value);
				return;
			}
		}
		if (selection !== undefined) {
			// Printable keys search (digits pick in short menus); Backspace edits; Esc clears first.
			const printable = data.length === 1 && data >= " " && data !== "\x7f" ? data : decodeKittyPrintable(data);
			if (printable !== undefined && printable !== " " ) {
				this.#selectQuery += printable;
				this.#rebuild();
				return;
			}
			if ((data === "\x7f" || data === "\b") && this.#selectQuery) {
				this.#selectQuery = this.#selectQuery.slice(0, -1);
				this.#rebuild();
				return;
			}
			if (data === "\x1b" && this.#selectQuery) {
				this.#selectQuery = "";
				this.#rebuild();
				return;
			}
		}
		this.#selectList?.handleInput(data);
	}""",
    },
    {
        "id": "client-tui-input-trace",
        "why": "OQT_DEBUG=1 records every input event with TUI state (busy, screen, selector) to a "
               "private log, to diagnose input that appears to be dropped in a user's terminal.",
        "file": "packages/coding-agent/src/experimental/client-tui.ts",
        "old": """	handleInput(data: string): void {
		if (this.#busy) {""",
        "new": """	handleInput(data: string): void {
		if (process.env.OQT_DEBUG && process.env.OQT_INPUT_TRACE)
			appendFileSync(
				process.env.OQT_INPUT_TRACE,
				`${new Date().toISOString()} busy=${this.#busy} screen=${this.#screen} select=${this.#selectList !== undefined} status=${JSON.stringify(this.#status)} data=${Buffer.from(data).toString("hex")}\n`,
			);
		if (this.#busy) {""",
    },
    {
        "id": "client-tui-input-trace-import",
        "why": "Import for the debug input trace.",
        "file": "packages/coding-agent/src/experimental/client-tui.ts",
        "old": """import { resolve } from "node:path";""",
        "new": """import { appendFileSync } from "node:fs";
import { resolve } from "node:path";""",
    },
    {
        "id": "client-tui-search-imports",
        "why": "fuzzyFilter and printable-key decoding for searchable selects.",
        "file": "packages/coding-agent/src/experimental/client-tui.ts",
        "old": """	CombinedAutocompleteProvider,
	type Component,
	Container,""",
        "new": """	CombinedAutocompleteProvider,
	type Component,
	Container,
	decodeKittyPrintable,
	fuzzyFilter,""",
    },
    {
        "id": "client-tui-model-cycle-keys",
        "why": "Classic pi model cycling: Ctrl+P / Shift+Ctrl+P run the oqt model-next/model-prev commands (scoped models first).",
        "file": "packages/coding-agent/src/experimental/client-tui.ts",
        "old": """		this.#chatInput.onAction("app.model.select", () => void this.#executeSlashCommand("model", ""));""",
        "new": """		this.#chatInput.onAction("app.model.select", () => void this.#executeSlashCommand("model", ""));
		this.#chatInput.onAction("app.model.cycleForward", () => void this.#executeSlashCommand("model-next", ""));
		this.#chatInput.onAction("app.model.cycleBackward", () => void this.#executeSlashCommand("model-prev", ""));""",
    },
    {
        "id": "client-tui-plugin-commands-first",
        "why": "Slash command registration is first-wins. oqt's single reviewed plugin must be able "
               "to provide /model (scoped, searchable) and /reload, so its facets register before "
               "pi's built-ins; the built-ins remain as fallbacks for everything else.",
        "file": "packages/coding-agent/src/experimental/client-tui.ts",
        "old": """				createSlashCommandsRuntimeFacet(),
				presentationBridgeFacet,
				createBuiltInSlashCommandsFacet({ reloadPresentationPlugins }),
				...this.#sharedFacets.facets,
				...presentationFacets.facets,""",
        "new": """				createSlashCommandsRuntimeFacet(),
				presentationBridgeFacet,
				...presentationFacets.facets,
				createBuiltInSlashCommandsFacet({ reloadPresentationPlugins }),
				...this.#sharedFacets.facets,""",
    },
    {
        "id": "client-tui-session-commands-facet",
        "why": "Classic-pi /new and /resume: an oqt client facet with a switcher bound to this TUI.",
        "file": "packages/coding-agent/src/experimental/client-tui.ts",
        "old": """				...presentationFacets.facets,
				createBuiltInSlashCommandsFacet({ reloadPresentationPlugins }),""",
        "new": """				...presentationFacets.facets,
				createOqtSessionFacet({
					currentSessionId: () => this.#sessionId,
					switchTo: (sessionId, management) => this.#switchSession(sessionId, management),
				}),
				createBuiltInSlashCommandsFacet({ reloadPresentationPlugins }),""",
    },
    {
        "id": "client-tui-switch-session",
        "why": "Switch the displayed Session: adopt the target id first (so the attachment and "
               "reattach handlers follow it), attach, wait for hydration, reopen the lane. "
               "Serialized with reconnect recovery.",
        "file": "packages/coding-agent/src/experimental/client-tui.ts",
        "old": """	#reattachAfterWorkerLoss(feature: SessionFeature, management: SessionManagement): void {""",
        "new": """	#switchSession(sessionId: string, management: SessionManagement): Promise<void> {
		const feature = this.#session;
		if (feature === undefined) return Promise.reject(new Error("No Session service is available"));
		return new Promise<void>((resolve, reject) => {
			this.#queueRecovery(async () => {
				try {
					this.#busy = true;
					this.#status = "Switching session…";
					this.#rebuild();
					this.#sessionId = sessionId;
					await this.#closeLane();
					// A Session left moments ago may still have a retiring worker that
					// rejects the attachment; retry until the server starts a fresh one.
					for (let attempt = 1; ; attempt++) {
						try {
							await management.attach(sessionId, BACKGROUND_CONTEXT);
							await Promise.race([
								feature.session.whenAttached(sessionId, BACKGROUND_CONTEXT),
								new Promise((_, timeout) =>
									setTimeout(() => timeout(new Error("Session did not attach in time")), 5_000),
								),
							]);
							break;
						} catch (error) {
							if (attempt >= 6) throw error;
							await new Promise((resolve) => setTimeout(resolve, 500 * attempt));
						}
					}
					await this.#openLane(feature);
					this.#busy = false;
					this.#status = "";
					this.#rebuild();
					resolve();
				} catch (error) {
					reject(error);
					throw error;
				}
			});
		});
	}

	#reattachAfterWorkerLoss(feature: SessionFeature, management: SessionManagement): void {""",
    },
    {
        "id": "client-tui-session-commands-import",
        "why": "Import the oqt session commands facet.",
        "file": "packages/coding-agent/src/experimental/client-tui.ts",
        "old": """import { appendFileSync } from "node:fs";""",
        "new": """import { appendFileSync } from "node:fs";
import { createOqtSessionFacet } from "./session-commands.ts";""",
    },
    {
        "id": "client-tui-attached-keeps-status",
        "why": "Attachment recovery cleared every status line, hiding e.g. the session-switch "
               "confirmation. Only clear the recovery's own messages.",
        "file": "packages/coding-agent/src/experimental/client-tui.ts",
        "old": """				if (this.#laneUnsubscribe === undefined) await this.#openLane(feature);
				this.#busy = false;
				this.#status = "";
				this.#rebuild();""",
        "new": """				if (this.#laneUnsubscribe === undefined) await this.#openLane(feature);
				this.#busy = false;
				if (/Reattaching|Reconnect|Session worker lost|Switching session/.test(this.#status)) this.#status = "";
				this.#rebuild();""",
    },
]


def apply(snapshot):

    applied = []
    for patch in PATCHES:
        path = snapshot / patch["file"]
        text = path.read_text()
        if text.count(patch["old"]) != 1:
            raise SystemExit(f"Upstream changed; refusing unreviewed patch {patch['id']}")
        path.write_text(text.replace(patch["old"], patch["new"]))
        applied.append(path)
    return applied
