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
			if (this.#selectorFor !== this.#selection || this.#selector === undefined) {
				const selector = new Container();
				selector.addChild(new Text(theme.bold(this.#selection.title), 1, 1));
				const items = [...this.#selection.items];
				this.#selectList = new SelectList(items, Math.min(Math.max(items.length, 1), 12), selectTheme);
				const selectedIndex = items.findIndex((item) => item.value === this.#selection?.selectedValue);
				if (selectedIndex >= 0) this.#selectList.setSelectedIndex(selectedIndex);
				this.#selectList.onSelect = (item) => this.#completeSelection(item.value);
				this.#selectList.onCancel = () => this.#completeSelection(undefined);
				selector.addChild(this.#selectList);
				this.#selector = selector;
				this.#selectorFor = this.#selection;
			}
			this.#editorContainer.addChild(this.#selector);
		} else {
			this.#selector = undefined;
			this.#selectorFor = undefined;""",
    },
    {
        "id": "client-tui-stable-selector-fields",
        "why": "State for the stable selector.",
        "file": "packages/coding-agent/src/experimental/client-tui.ts",
        "old": """	readonly #sessionHeading = new Text("", 1, 0);""",
        "new": """	readonly #sessionHeading = new Text("", 1, 0);
	#selector: Container | undefined;
	#selectorFor: unknown;""",
    },
    {
        "id": "client-tui-select-number-keys",
        "why": "Number keys pick a selector item directly (1-9).",
        "file": "packages/coding-agent/src/experimental/client-tui.ts",
        "old": """		this.#selectList?.handleInput(data);
	}""",
        "new": """		const digit = /^[1-9]$/.test(data) ? Number(data) - 1 : -1;
		const item = digit >= 0 ? this.#selection?.items[digit] : undefined;
		if (item !== undefined) {
			this.#completeSelection(item.value);
			return;
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
