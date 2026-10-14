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
