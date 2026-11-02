import type { ProviderBinding } from "@oqto-flash/db";
import { DEFAULT_OQTO_UI_CONFIG, type OqtoUiPlatform } from "./contracts";
import type { FileChange } from "./files-contract";
import { createFlashChat } from "./flash-chat";
import { createFlashHosts } from "./flash-hosts";

/** Browser execution changes the host ports, never the shared Oqto views. */
export async function createFlashPlatform(
	binding: ProviderBinding,
	apiKey: string,
): Promise<OqtoUiPlatform & { dispose(): Promise<void> }> {
	const runtime = createFlashChat({ binding, apiKey });
	try {
		await runtime.open();
	} catch (error) {
		await runtime.dispose();
		throw error;
	}
	const identity = runtime.getIdentity();
	const fileListeners = new Set<(event: FileChange) => void>();
	let filesRevision = runtime.getFilesRevision();
	const unwatch = runtime.subscribe(() => {
		const next = runtime.getFilesRevision();
		if (next === filesRevision) return;
		filesRevision = next;
		for (const listener of fileListeners)
			listener({ kind: "resync", path: "", directory: true });
	});
	return {
		id: `flash:${identity.publicSessionId}`,
		...createFlashHosts({
			list: (path) => runtime.list(path),
			read: (path) => runtime.read(path),
			subscribe(listener) {
				fileListeners.add(listener);
				return () => {
					fileListeners.delete(listener);
				};
			},
		}),
		chat: runtime.chat,
		selectModel: (id, modelId) => runtime.selectModel(id, modelId),
		async createSession(
			workDirectoryId,
			sourceSessionId = identity.publicSessionId,
		) {
			if (workDirectoryId !== identity.workDirectoryId)
				throw new Error("Unknown browser workspace");
			const created = await runtime.createSession(sourceSessionId);
			return {
				sessionId: created.publicSessionId,
				workDirectoryId: created.workDirectoryId,
			};
		},
		loadUiConfig: async () => structuredClone(DEFAULT_OQTO_UI_CONFIG),
		loadMessages: (id, before, limit) =>
			runtime.loadMessages(id, before, limit),
		async load(requestedId) {
			const catalog = await runtime.sessions();
			const activeId = requestedId ?? identity.publicSessionId;
			if (!catalog.some(({ publicSessionId }) => publicSessionId === activeId))
				throw new Error("Unknown browser Session");
			const view = await runtime.ensureSession(activeId);
			if (!view)
				throw new Error("Native browser Session snapshot is unavailable");
			return {
				workDirectories: [
					{
						id: identity.workDirectoryId,
						name: "Browser workspace",
						path: "/workspace",
						accent: "var(--oq-primary)",
						sessions: catalog.map((session) => ({
							id: session.publicSessionId,
							name: session.name,
							preview: "",
							updated: "",
							status: session.status,
							model: session.modelId,
						})),
					},
				],
				activeSessionId: activeId,
				files: [],
				workArea: {
					tabs: [
						{ id: "chat", owner: "session" },
						{ id: "editor", owner: "workDirectory" },
						{ id: "terminal", owner: "workDirectory" },
					],
					editorLines: [],
					terminalLines: [],
				},
				gallery: [],
				environment: {
					models: (binding.models ?? [{ id: binding.model }]).map(({ id }) => ({
						id,
						name: id,
					})),
					connection: "connected",
					statusBar: {
						runningSessions: String(
							catalog.filter(({ status }) => status === "working").length,
						),
						onlineUsers: "1",
						runnerLoad: "Browser",
						version: "Flash · Durable 1.3",
					},
				},
			};
		},
		dispose: async () => {
			unwatch();
			fileListeners.clear();
			await runtime.dispose();
		},
	};
}
