"""Exact, reviewed edits to the private upstream derivative.

Each `old` must occur exactly once in the pinned source or preparation fails closed.
Keep this list small; every entry is a candidate upstream fix.
"""

PATCHES = [
    {
        "id": "client-tui-bang-commands",
        "why": "Classic Pi runs `!cmd` (and `!!cmd`, kept out of context) as the user's own "
               "command. Route it to the oqt-shell plugin command, which runs it in the confined "
               "hands via the operator socket; results are passively persisted by the owning worker.",
        "file": "packages/coding-agent/src/experimental/client-tui.ts",
        "old": """		if (prompt.length === 0) return;
		if (prompt.startsWith("/")) {""",
        "new": """		if (prompt.length === 0) return;
		if (prompt.startsWith("!") && this.#sessionId !== undefined) {
			await this.#executeSlashCommand("oqt-shell", `${this.#sessionId}\\n${prompt}`);
			return;
		}
		if (prompt.startsWith("/")) {""",
    },
    {
        "id": "client-tui-operator-receipt-inspect",
        "why": "Capture the selected Session for durable result inspection; never infer it from visible order.",
        "file": "packages/coding-agent/src/experimental/client-tui.ts",
        "old": """		if (prompt.startsWith("/")) {""",
        "new": """		if (prompt === "/shell-results" && this.#sessionId !== undefined) {
			await this.#executeSlashCommand("oqt-shell-results", this.#sessionId);
			return;
		}
		if (prompt.startsWith("/")) {""",
    },
    {
        "id": "worker-before-harness-close-contract",
        "why": "Dispose receipt listeners and drain native writes before their owning Harness closes.",
        "file": "packages/coding-agent/src/experimental/services/worker.ts",
        "old": """	/** Release resources the Harness does not own, such as execution environments, after it closed. */
	cleanup?(context: Context): Promise<void>;""",
        "new": """	/** Stop producers and drain their native writes before closing the owning Harness. */
	beforeClose?(context: Context): Promise<void>;
	/** Release resources the Harness does not own, such as execution environments, after it closed. */
	cleanup?(context: Context): Promise<void>;""",
    },
    {
        "id": "worker-before-harness-close-lifecycle",
        "why": "Explicit lifecycle seam, not a monkeypatch of Harness.close; failures still allow cleanup.",
        "file": "packages/coding-agent/src/experimental/session-worker.ts",
        "old": """	try {
		await resources.runtime?.harness.close(TODO_CONTEXT);""",
        "new": """	try {
		await resources.runtime?.beforeClose?.(TODO_CONTEXT);
	} catch (error) {
		errors.push(error);
	}
	try {
		await resources.runtime?.harness.close(TODO_CONTEXT);""",
    },
    {
        "id": "server-idle-grace-option",
        "why": "An auto server retired 1 s after its last client left, so every oqt launch "
               "paid a cold host + Session worker start. Let the host keep a warm idle grace.",
        "file": "packages/coding-agent/src/experimental/server.ts",
        "old": """	constructor(keepAlive: boolean) {
		this.#keepAlive = keepAlive;
		this.#startupHeld = !keepAlive;
	}""",
        "new": """	readonly #idleGraceMs: number;

	constructor(keepAlive: boolean, idleGraceMs = AUTO_SERVER_IDLE_GRACE_MS) {
		this.#keepAlive = keepAlive;
		this.#startupHeld = !keepAlive;
		this.#idleGraceMs = idleGraceMs;
	}""",
    },
    {
        "id": "server-idle-grace-use",
        "why": "Use the configured idle grace (see server-idle-grace-option).",
        "file": "packages/coding-agent/src/experimental/server.ts",
        "old": """				retire();
			}
		}, AUTO_SERVER_IDLE_GRACE_MS);""",
        "new": """				retire();
			}
		}, this.#idleGraceMs);""",
    },
    {
        "id": "server-idle-grace-field",
        "why": "Expose the idle grace on StartServerOptions (see server-idle-grace-option).",
        "file": "packages/coding-agent/src/experimental/server.ts",
        "old": """	readonly keepAlive?: boolean;
	/** Optional explicit Radius credential.""",
        "new": """	readonly keepAlive?: boolean;
	/** How long an auto server lingers without clients or workers (default 1 s). */
	readonly idleGraceMs?: number;
	/** Optional explicit Radius credential.""",
    },
    {
        "id": "server-idle-grace-pass",
        "why": "Pass the idle grace to the lifetime (see server-idle-grace-option).",
        "file": "packages/coding-agent/src/experimental/server.ts",
        "old": """	const lifetime = new ServerLifetime(options.keepAlive ?? true);""",
        "new": """	const lifetime = new ServerLifetime(options.keepAlive ?? true, options.idleGraceMs);""",
    },
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
        "new": """import { spawnOqtSessionWorker, registerOqtInputAttachment, dropOqtInputAttachment, forgetOqtInputAttachment, beforeOqtInputCall, assertOqtInputCall, afterOqtInputCall } from "./spawn-hook.ts";""",
    },
    {
        "id": "session-input-register-native-scope",
        "why": "Bind input authority to coordinator-generated attachment IDs; cancellation uses only recorded native submission IDs.",
        "file": "packages/coding-agent/src/experimental/session-worker-manager.ts",
        "old": """		const scope = this.#operationScope(worker, attachmentId);
		let released = false;""",
        "new": """		const scope = this.#operationScope(worker, attachmentId);
		registerOqtInputAttachment(worker.metadata.id, attachmentId, async (id) => {
			await this.#invoke(worker, scope, { serviceId: "pi.agent-controller", member: "cancelQueued", args: [id] }, BACKGROUND_CONTEXT);
		}, () => this.#stopWorker(worker, BACKGROUND_CONTEXT));
		let released = false;""",
    },
    {
        "id": "session-input-dispose-native-scope",
        "why": "Detachment invalidates pending confirmation and cancels this attachment's own queued input before forgetting its authority.",
        "file": "packages/coding-agent/src/experimental/session-worker-manager.ts",
        "old": """				} finally {
					worker.attachmentIds.delete(attachmentId);
					this.#removeServiceSubscriptions""",
        "new": """				} finally {
					await dropOqtInputAttachment(worker.metadata.id, attachmentId);
					worker.attachmentIds.delete(attachmentId);
					this.#removeServiceSubscriptions""",
    },
    {
        "id": "session-input-gate-before-forward",
        "why": "The trusted host checks input ownership before forwarding a mutation into the confined brain.",
        "file": "packages/coding-agent/src/experimental/session-worker-manager.ts",
        "old": """		const control = decodeServiceControlCall(call);
		let addedSubscriptionKey""",
        "new": """		const inputAdmission = await beforeOqtInputCall(worker.metadata.id, scope.attachmentId, call);
		const control = decodeServiceControlCall(call);
		let addedSubscriptionKey""",
    },
    {
        "id": "session-input-synchronous-dispatch-fence",
        "why": "Revalidate after awaited admission and immediately before native send; a stale caller must not interrupt the worker.",
        "file": "packages/coding-agent/src/experimental/session-worker-manager.ts",
        "old": """		let response: JsonValue | undefined;
		try {
			response = await this.#invoke(worker, scope, call, context);""",
        "new": """		let response: JsonValue | undefined;
		assertOqtInputCall(worker.metadata.id, inputAdmission);
		try {
			response = await this.#invoke(worker, scope, call, context);""",
    },
    {
        "id": "session-input-record-native-receipt",
        "why": "Correlate native submission IDs, never message text/order. A revoke racing admission cancels only recorded remote input.",
        "file": "packages/coding-agent/src/experimental/session-worker-manager.ts",
        "old": """		if (control?.type === "unsubscribe") {
			this.#serviceSubscriptions.delete(scopedServiceSubscriptionKey(scope, control.subscriptionId));
		}
		return response;""",
        "new": """		if (control?.type === "unsubscribe") {
			this.#serviceSubscriptions.delete(scopedServiceSubscriptionKey(scope, control.subscriptionId));
		}
		await afterOqtInputCall(worker.metadata.id, inputAdmission, call, response);
		return response;""",
    },
    {
        "id": "session-input-retire-worker-bindings",
        "why": "Worker recovery retires old attachment authority instead of carrying a lease into a replacement process.",
        "file": "packages/coding-agent/src/experimental/session-worker-manager.ts",
        "old": """		if (this.#workersByPeer.get(worker.peerId) !== worker) return;
		this.#rejectWorkerOperations(worker,""",
        "new": """		if (this.#workersByPeer.get(worker.peerId) !== worker) return;
		for (const attachment of worker.attachmentIds) forgetOqtInputAttachment(worker.metadata.id, attachment);
		this.#rejectWorkerOperations(worker,""",
    },
    {
        "id": "session-input-uncertain-admission-fails-closed",
        "why": "A lost admission response cannot identify a queued submission safely; stop only the owned worker, never retry the input.",
        "file": "packages/coding-agent/src/experimental/session-worker-manager.ts",
        "old": """		} catch (error) {
			if (addedSubscriptionKey !== undefined && control?.type === "subscribe") {""",
        "new": """		} catch (error) {
			if (inputAdmission?.remote) await this.#stopWorker(worker, BACKGROUND_CONTEXT);
			if (addedSubscriptionKey !== undefined && control?.type === "subscribe") {""",
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
