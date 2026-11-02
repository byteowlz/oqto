/** Owned presentation lifecycle over the pinned transport adapter. This seam
 * can replace the retired experimental transport without changing the editor,
 * Durable storage, or guard authority. Startup dispatches at most one queued
 * message, only to the exact attached target (see presentation-startup.ts).
 *
 * Static imports stay limited to what the first editable frame needs; the
 * client/server stack loads behind the shell while the host starts.
 */
import { randomUUID } from "node:crypto";
import type { ClientCommand } from "../cli/experimental/commands/client.ts";
import { getAgentDir } from "../config.ts";
import { SettingsManager } from "../core/settings-manager.ts";
import {
	initTheme,
	setRegisteredThemes,
	stopThemeWatcher,
} from "../modes/interactive/theme/theme.ts";
import { createInteractiveTui } from "../modes/interactive/tui-renderer.ts";
import type { OpenClientRuntimeOptions } from "./client-runtime.ts";
import type { ExperimentalClientTui } from "./client-tui.ts";
import { StartupShell } from "./presentation-shell.ts";
import {
	createPresentationStartup,
	type StartupSubmission,
} from "./presentation-startup.ts";
import { mark } from "./trace.ts";

export interface PreparedPresentation {
	readonly command: ClientCommand;
	/** Attached host identity bound into any queued startup message. */
	readonly host: string;
}

type Dispatch = (
	submission: StartupSubmission,
) => Promise<"accepted" | "rejected">;

/** Heaviest leaves of the client stack, loaded one per event-loop turn so the
 * shell keeps echoing keystrokes while evaluation runs (module evaluation is
 * synchronous). Order follows the measured cost; paths are type-checked. */
const PRELOAD = [
	() => import("@earendil-works/chord"),
	() => import("../core/model-runtime.ts"),
	() => import("./radius-auth.ts"),
	() => import("./plugins/package.ts"),
	() => import("./session-catalog.ts"),
	() => import("./session-worker-manager.ts"),
	() => import("./server.ts"),
	() => import("./client-runtime.ts"),
	() => import("../core/resource-loader.ts"),
	() => import("./client-tui.ts"),
] as const;
/** One frame: lets the first render and terminal handshake complete. */
const SETTLE_MS = 16;
const yieldToInput = () =>
	new Promise<void>((resolve) => setImmediate(resolve));

/** Mount the native editor before loading the client stack, host preparation,
 * discovery, or attachment. Exiting fences late initialization. Failed
 * initialization keeps the editor and its text until the user exits; it never
 * selects a different Session or starts an inference to test readiness.
 */
export async function runEarlyClientTui(options: {
	readonly runtime: string;
	readonly prepare: (signal: AbortSignal) => Promise<PreparedPresentation>;
	/** Human-readable cause for a failed start (e.g. from the private host log). */
	readonly explainFailure?: (error: unknown) => Promise<string>;
	readonly client?: OpenClientRuntimeOptions;
}): Promise<{ readonly failure?: string }> {
	const { runtime, prepare } = options;
	let failure: string | undefined;
	const settings = SettingsManager.create(process.cwd(), getAgentDir());
	initTheme();
	const ui = createInteractiveTui({
		tuiMode: "fullscreen",
		showHardwareCursor: settings.getShowHardwareCursor(),
		logDirectory: getAgentDir(),
		fullscreenWheelScrollLines: settings.getFullscreenWheelScrollLines(),
	});
	ui.setClearOnShrink(settings.getClearOnShrink());
	const abort = new AbortController();
	let finish: () => void = () => {};
	let closed = false;
	const finished = new Promise<void>((resolve) => {
		finish = () => {
			if (closed) return;
			closed = true;
			abort.abort();
			resolve();
		};
	});
	// One queue survives the shell -> full TUI handoff. Before the handoff it
	// renders through the shell and cannot dispatch (ready() is never called).
	const relay: { dispatch?: Dispatch; changed: () => void } = {
		changed: () => shell.refresh(),
	};
	const queue = createPresentationStartup({
		launchId: randomUUID(),
		runtime,
		dispatch: (submission) =>
			relay.dispatch ? relay.dispatch(submission) : Promise.resolve("rejected"),
		changed: () => relay.changed(),
	});
	const shell = new StartupShell(ui, queue, () => finish());
	let component: ExperimentalClientTui | undefined;
	let themes:
		| import("../modes/interactive/theme/theme-controller.ts").InteractiveThemeController
		| undefined;
	let client: { dispose(): Promise<void> } | undefined;
	let started = false;
	const reportFailure = async (error: unknown) => {
		if (closed) return;
		const plain = error instanceof Error ? error.message : String(error);
		failure = await (
			options.explainFailure?.(error) ?? Promise.resolve(plain)
		).catch(() => plain);
		if (closed) return;
		if (component) component.failStartup(failure);
		else shell.fail(failure);
	};
	const initialize = async () => {
		const host = prepare(abort.signal);
		host.catch(() => {}); // Awaited below; never an unhandled rejection.
		try {
			// Let the first frame and terminal handshake flush before evaluation
			// (synchronous) starts occupying the event loop.
			await new Promise<void>((resolve) => setTimeout(resolve, SETTLE_MS));
			for (const load of PRELOAD) {
				await load();
				if (closed) return;
				await yieldToInput();
			}
			const [
				{ ExperimentalClientTui },
				{ openClientRuntime },
				{ DefaultResourceLoader },
				{ InteractiveThemeController },
			] = await Promise.all([
				import("./client-tui.ts"),
				import("./client-runtime.ts"),
				import("../core/resource-loader.ts"),
				import("../modes/interactive/theme/theme-controller.ts"),
			]);
			if (closed) return;
			mark("presentation: client stack loaded");
			const full = await ExperimentalClientTui.createPending({
				ui,
				runtime,
				requestRender: () => ui.requestRender(),
				finish: () => finish(),
				startup: {
					queue,
					bind: (dispatch, changed) => {
						relay.dispatch = dispatch;
						relay.changed = changed;
					},
				},
			});
			if (closed) {
				await full.close();
				return;
			}
			// Synchronous handoff: no input can arrive between reading the shell
			// draft and focusing the full editor.
			const draft = shell.editor.getText();
			ui.removeChild(shell);
			ui.addChild(full);
			ui.setLayoutRoot(full.layoutRoot);
			ui.setFocus(full);
			full.adoptStartupDraft(draft);
			component = full;
			ui.requestRender();
			themes = new InteractiveThemeController(ui, {
				getSettingsManager: () => settings,
				showError: (error) => full.showError(error),
				onChanged: () => full.refreshTheme(),
			});
			const prepared = await host;
			if (closed) return;
			mark("presentation: host ready");
			const loader = new DefaultResourceLoader({
				cwd: process.cwd(),
				agentDir: getAgentDir(),
				settingsManager: settings,
				noExtensions: true,
				noSkills: true,
				noPromptTemplates: true,
				noContextFiles: true,
			});
			await loader.reload();
			if (closed) return;
			setRegisteredThemes(loader.getThemes().themes);
			themes.applyFromSettings();
			const opened = await openClientRuntime(prepared.command, options.client);
			client = opened;
			if (closed) return;
			await full.attachPrepared(
				prepared.command,
				opened.servers.map((server) => ({
					serverId: server.route.serverId,
					radius: server.route.transport === "radius",
					server: server.server,
					session: server.session,
				})),
				prepared.host,
			);
			mark("presentation: attached");
		} catch (error) {
			await reportFailure(error);
		}
	};
	let initialization: Promise<void> | undefined;
	try {
		ui.addChild(shell);
		ui.setLayoutRoot(shell.layoutRoot);
		ui.setFocus(shell.editor);
		ui.start();
		started = true;
		ui.renderNow();
		mark("presentation: first editable frame");
		initialization = initialize();
		await finished;
	} finally {
		closed = true;
		abort.abort();
		themes?.disableAutoSync();
		if (started) ui.stop();
		// Serialize disposal after initialization: never close a half-constructed
		// facet host while another continuation is installing its subscriptions.
		await initialization;
		queue.close();
		themes?.dispose();
		stopThemeWatcher();
		await component?.close();
		await client?.dispose();
	}
	return failure === undefined ? {} : { failure };
}
