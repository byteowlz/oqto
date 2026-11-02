// The oqt TUI on host2 (ADR-0071). Derived from Pi's experimental client-tui.ts
// at 2ee5c1e5 (MIT, Mario Zechner) plus oqt's reviewed patches (stable
// selector, number keys, searchable menus, classic chrome/footer, model cycling,
// early editor, program status). Talks only to the server facade; plugins are
// bundled first-party modules on the local API in tui-api.ts.

import { randomUUID } from "node:crypto";
import { appendFileSync } from "node:fs";
import { type Context, isChordError } from "@earendil-works/chord";
import { BACKGROUND_CONTEXT } from "@earendil-works/chord/context";
import type { AgentState, ConversationView } from "@earendil-works/pi-durable";
import {
	CombinedAutocompleteProvider,
	type Component,
	Container,
	decodeKittyPrintable,
	fuzzyFilter,
	type SelectItem,
	SelectList,
	setKeybindings,
	Text,
	type TUI,
} from "@earendil-works/pi-tui";
import { KeybindingsManager } from "../core/keybindings.ts";
import type {
	createPresentationStartup,
	StartupSubmission,
} from "../experimental/presentation-startup.ts";
import { programStatus } from "../experimental/program-status.ts";
import { createChatViewport } from "../modes/interactive/chat-viewport.ts";
import { CustomEditor } from "../modes/interactive/components/custom-editor.ts";
import { formatTokens } from "../modes/interactive/components/footer.ts";
import { getEditorTheme, theme } from "../modes/interactive/theme/theme.ts";
import type { OqtClient } from "./client.ts";
import { Conversation, Session, Sessions } from "./contract.ts";
import {
	createToolPresentation,
	type ParticipantRef,
	type SessionRef,
	type SessionsRef,
	SlashCommandRegistry,
	type SlashCommandRunResult,
	type TuiCurrent,
	type TuiPlugin,
	type TuiPluginApi,
} from "./tui-api.ts";
import { ExperimentalChatView, liveOf } from "./tui-chat.ts";

interface PendingSelection {
	readonly title: string;
	readonly items: readonly SelectItem[];
	readonly selectedValue?: string;
	resolve(value: string | undefined): void;
}

const selectTheme = {
	selectedPrefix: (text: string) => theme.fg("accent", text),
	selectedText: (text: string) => theme.fg("accent", text),
	description: (text: string) => theme.fg("muted", text),
	scrollInfo: (text: string) => theme.fg("dim", text),
	noMatch: (text: string) => theme.fg("warning", text),
};
const THINKING_LABELS: Record<string, string> = {
	off: "No reasoning",
	minimal: "Minimal reasoning",
	low: "Light reasoning",
	medium: "Balanced reasoning",
	high: "Deep reasoning",
	xhigh: "Extra-high reasoning",
	max: "Maximum reasoning",
};
const message = (error: unknown) =>
	error instanceof Error ? error.message : String(error);
/** A call that may have reached the host before its link dropped. */
const linkLost = (error: unknown) =>
	isChordError(error) &&
	(error.code === "unavailable" || error.code === "gone");

type StartupQueue = ReturnType<typeof createPresentationStartup>;

export interface OqtTuiOptions {
	readonly ui: TUI;
	/** Runtime identity bound into a queued startup message. */
	readonly runtime: string;
	readonly plugins: readonly TuiPlugin[];
	/** The startup queue created with the first frame; adopted, never replaced. */
	readonly startup?: {
		readonly queue: StartupQueue;
		bind(
			dispatch: (
				submission: StartupSubmission,
			) => Promise<"accepted" | "rejected">,
			changed: () => void,
		): void;
	};
	requestRender(): void;
	finish(): void;
}

export class OqtTui implements Component {
	readonly #ui: TUI;
	readonly #requestRender: () => void;
	readonly #finish: () => void;
	readonly #runtime: string;
	readonly #documentContainer = new Container();
	#selector: Container | undefined;
	#selectorFor: unknown;
	#selectorQuery = "";
	#selectQuery = "";
	readonly #pendingMessagesContainer = new Container();
	readonly #statusContainer = new Container();
	readonly #editorContainer = new Container();
	readonly #footerComponent = new Text("", 1, 0);
	readonly #layoutRoot: Component;
	readonly #keybindings = KeybindingsManager.create();
	readonly #commands = new SlashCommandRegistry();
	readonly #toolPresentation = createToolPresentation();
	readonly #listeners = new Set<() => void>();
	readonly #pluginCleanups: (() => void)[] = [];
	readonly #chatInput: CustomEditor;
	#client: OqtClient | undefined;
	#sessions: SessionsRef | undefined;
	#sessionId: string | undefined;
	#session: SessionRef | undefined;
	#conversation: ParticipantRef | undefined;
	#unsubscribe: (() => void)[] = [];
	#chatView: ExperimentalChatView | undefined;
	#selectList: SelectList | undefined;
	#selection: PendingSelection | undefined;
	#screen: "select" | "chat" = "chat";
	#status = "Starting Session…";
	#startupPending = false;
	#startupNotice = "";
	#startup: StartupQueue | undefined;
	#busy = false;
	#closed = false;
	#closePromise: Promise<void> | undefined;
	/** Serializes attach/switch/recover so two transitions never interleave. */
	#transition: Promise<void> = Promise.resolve();

	constructor(options: OqtTuiOptions) {
		this.#ui = options.ui;
		this.#runtime = options.runtime;
		programStatus.attach(options.ui.terminal);
		this.#requestRender = options.requestRender;
		this.#finish = options.finish;
		setKeybindings(this.#keybindings);
		this.#chatInput = new CustomEditor(
			options.ui,
			getEditorTheme(),
			this.#keybindings,
			{ paddingX: 1 },
		);
		this.#chatInput.onSubmit = (text) => void this.#runPrompt(text);
		this.#chatInput.onEscape = () => this.#interrupt();
		this.#chatInput.onCtrlD = options.finish;
		this.#chatInput.onAction("app.clear", options.finish);
		this.#chatInput.onAction(
			"app.model.select",
			() => void this.#executeSlashCommand("model", ""),
		);
		this.#chatInput.onAction(
			"app.model.cycleForward",
			() => void this.#executeSlashCommand("model-next", ""),
		);
		this.#chatInput.onAction(
			"app.model.cycleBackward",
			() => void this.#executeSlashCommand("model-prev", ""),
		);
		this.#chatInput.onAction("app.message.followUp", () => {
			const text = this.#chatInput.getText().trim();
			if (text.length === 0) return;
			this.#chatInput.setText("");
			void this.#queueFollowUp(text);
		});
		this.#editorContainer.addChild(this.#chatInput);
		this.#layoutRoot = createChatViewport({
			document: this.#documentContainer,
			pendingMessages: this.#pendingMessagesContainer,
			status: this.#statusContainer,
			editor: this.#editorContainer,
			footer: this.#footerComponent,
			scrollbarTrackStyle: (text) => theme.fg("scrollbarTrack", text),
			scrollbarThumbStyle: (text) => theme.fg("scrollbarThumb", text),
		}).root;
		if (options.startup) {
			this.#startupPending = true;
			this.#startup = options.startup.queue;
			options.startup.bind(
				(submission) => this.#dispatchStartup(submission),
				() => this.#startupChanged(),
			);
		}
		this.#commands.subscribe(() => this.#updateAutocomplete());
		this.#registerBuiltIns();
		const api = this.#pluginApi();
		// Plugins first: their reviewed commands win over built-ins of the same name.
		for (const plugin of options.plugins)
			this.#pluginCleanups.push(plugin(api));
		this.#rebuild();
	}

	// ─── Plugin API ──────────────────────────────────────────────────────────

	#pluginApi(): TuiPluginApi {
		return {
			ui: {
				select: (title, items, selectedValue) =>
					this.#select(title, items, selectedValue),
				showStatus: (status) => {
					this.#status = status;
					this.#rebuild();
				},
			},
			commands: this.#commands,
			tools: this.#toolPresentation,
			current: () => this.#current(),
			onChange: (listener) => {
				this.#listeners.add(listener);
				return () => this.#listeners.delete(listener);
			},
			sessions: () => this.#sessions,
			switchSession: (session) => this.#queue(() => this.#attach(session)),
			newSession: () =>
				this.#queue(async () => {
					const sessions = this.#sessions;
					if (!sessions) throw new Error("Sessions are unavailable");
					await this.#attach((await sessions.create()).session);
				}),
		};
	}

	#current(): TuiCurrent | undefined {
		const view = this.#view();
		if (!this.#sessionId || !this.#session || !this.#conversation || !view)
			return undefined;
		return {
			session: this.#sessionId,
			sessionRef: this.#session,
			conversation: this.#conversation,
			view,
		};
	}

	#notify(): void {
		for (const listener of this.#listeners) {
			try {
				listener();
			} catch {
				/* a plugin failure never breaks the TUI */
			}
		}
	}

	// ─── Attachment ──────────────────────────────────────────────────────────

	/** Connects to the server and opens a Session: `session`, the newest with
	 * `continue`, or a new one. Releases at most one queued startup message to
	 * the exact attached target. */
	async open(
		client: OqtClient,
		options: { readonly session?: string; readonly continue?: boolean },
		host: string,
	): Promise<void> {
		this.#client = client;
		const sessions = await client.host.get(Sessions, {});
		if (this.#closed) return sessions.release();
		this.#sessions = sessions;
		const target =
			options.session ??
			(options.continue ? sessions.sessions.value[0]?.id : undefined) ??
			(await sessions.create()).session;
		await this.#queue(() => this.#attach(target));
		if (this.#closed) return;
		const startup = this.#startup;
		const current = this.#current();
		if (startup !== undefined) {
			const agent = (current?.view.docs["pi.agent"] ?? {}) as AgentState;
			const model =
				agent.model === undefined
					? ""
					: `${agent.model.provider}/${agent.model.modelId}`;
			if (!current || !model) {
				this.#startupNotice = current
					? "Queued message not sent: no model is configured"
					: "Queued message not sent: no Session";
				startup.fail();
			} else {
				try {
					await startup.ready({
						runtime: this.#runtime,
						session: current.session,
						conversation: String(current.view.conversation.id),
						model,
						host,
					});
				} catch (error) {
					this.#startupNotice = `Queued message not sent: ${message(error)}`;
					startup.fail();
				}
			}
		}
		this.#startupPending = false;
		this.#startupChanged();
	}

	#queue(operation: () => Promise<void>): Promise<void> {
		const run = this.#transition.then(async () => {
			if (!this.#closed) await operation();
		});
		this.#transition = run.catch((error: unknown) => {
			if (this.#closed) return;
			this.#busy = true;
			this.#status = `Reconnect error: ${message(error)}`;
			this.#rebuild();
		});
		return run;
	}

	/** Attaches a Session and its selected conversation (inside #queue). */
	async #attach(session: string): Promise<void> {
		const client = this.#client;
		if (!client) throw new Error("Not connected");
		this.#busy = true;
		this.#status =
			this.#sessionId && this.#sessionId !== session
				? "Switching session…"
				: "Attaching Session…";
		this.#rebuild();
		this.#detach();
		this.#sessionId = session;
		const ref = await client.host.get(Session, { session });
		if (this.#closed || this.#sessionId !== session) return ref.release();
		this.#session = ref;
		this.#unsubscribe.push(
			ref.onStatus((status) => this.#onLinkStatus(status)),
			// Another client (or our /tree) selected another conversation: follow it.
			ref.selected.subscribe((selected) => {
				if (
					this.#conversation &&
					Number(this.#view()?.conversation.id) !== selected
				)
					void this.#queue(() => this.#openConversation(selected));
			}),
		);
		await this.#openConversation(ref.selected.value);
		this.#busy = false;
		if (
			/Attaching|Switching|Reattaching|Reconnect|worker lost/.test(this.#status)
		)
			this.#status = "";
		this.#rebuild();
	}

	async #openConversation(conversation: number): Promise<void> {
		const client = this.#client;
		const session = this.#sessionId;
		if (!client || !session) return;
		const ref = await client.host.get(Conversation, { session, conversation });
		if (this.#closed || this.#sessionId !== session) return ref.release();
		if (ref.variant !== "participant") {
			ref.release();
			throw new Error("This client may only view the Session");
		}
		this.#closeLane();
		this.#conversation = ref;
		const view = new ExperimentalChatView(
			this.#ui,
			process.cwd(),
			this.#toolPresentation,
		);
		this.#chatView = view;
		this.#documentContainer.addChild(view.transcript);
		this.#pendingMessagesContainer.addChild(view.pendingMessages);
		this.#unsubscribe.push(
			ref.onStatus((status) => this.#onLinkStatus(status)),
			ref.view.subscribe((value) => {
				if (this.#conversation !== ref) return;
				view.apply(value as ConversationView);
				this.#rebuild();
				this.#notify();
			}),
		);
		this.#updateAutocomplete();
		this.#notify();
	}

	/** References go `loading` while a link reconnects and `gone` when their
	 * object was retired (worker lost, revocation): re-acquire the Session. */
	#onLinkStatus(status: string): void {
		if (this.#closed || !this.#sessionId) return;
		if (status === "loading") {
			this.#busy = true;
			this.#status = "Reattaching Session…";
			this.#rebuild();
		} else if (status === "live") {
			this.#busy = false;
			if (/Reattaching/.test(this.#status)) this.#status = "";
			this.#rebuild();
		} else if (status === "gone") {
			const session = this.#sessionId;
			this.#busy = true;
			this.#status = "Session worker lost; recovering the Session…";
			this.#rebuild();
			void this.#queue(async () => {
				if (this.#sessionId !== session) return;
				for (let attempt = 1; ; attempt++) {
					try {
						await this.#attach(session);
						return;
					} catch (error) {
						if (attempt >= 6 || this.#closed) throw error;
						await new Promise((resolve) => setTimeout(resolve, 500 * attempt));
					}
				}
			});
		}
	}

	#closeLane(): void {
		this.#conversation?.release();
		this.#conversation = undefined;
		this.#chatView?.dispose();
		this.#chatView = undefined;
		this.#documentContainer.clear();
		this.#pendingMessagesContainer.clear();
		this.#statusContainer.clear();
	}

	#detach(): void {
		for (const unsubscribe of this.#unsubscribe.splice(0)) unsubscribe();
		this.#closeLane();
		this.#session?.release();
		this.#session = undefined;
	}

	// ─── Startup queue (ADR-0070) ───────────────────────────────────────────

	adoptStartupDraft(text: string): void {
		if (text.length > 0) this.#chatInput.setText(text);
		this.#startupChanged();
	}

	/** Startup could not reach a Session: keep the editor and draft, send nothing. */
	failStartup(reason: string): void {
		if (this.#closed) return;
		this.#startupNotice = `Startup failed; nothing was sent: ${reason}`;
		if (this.#startup === undefined) {
			this.#status = this.#startupNotice;
			this.#rebuild();
		} else this.#startup.fail();
	}

	async #dispatchStartup(
		submission: StartupSubmission,
	): Promise<"accepted" | "rejected"> {
		const current = this.#current();
		if (
			this.#closed ||
			!current ||
			current.session !== submission.binding.session ||
			String(current.view.conversation.id) !== submission.binding.conversation
		) {
			this.#startupNotice =
				"Queued message not sent: the attached conversation changed";
			return "rejected";
		}
		// The startup request id makes a retry after a lost reply exactly-once.
		const result = await this.#send(
			current.conversation,
			submission.text,
			submission.requestId.replace(/[^A-Za-z0-9_.:-]/g, "-"),
		);
		if (!result.accepted)
			this.#startupNotice = `Queued message not accepted: ${result.error}`;
		return result.accepted ? "accepted" : "rejected";
	}

	#startupChanged(): void {
		const startup = this.#startup;
		if (startup === undefined) {
			if (this.#startupNotice) this.#status = this.#startupNotice;
			this.#rebuild();
			return;
		}
		const view = startup.input.view();
		let restored = false;
		for (const item of view.retained) {
			if (this.#chatInput.getText().length > 0) break;
			this.#chatInput.setText(item.text);
			startup.input.release(item.requestId);
			restored = true;
		}
		const kept = startup.input.view().retained.length;
		const suffix = restored
			? " — text restored to the editor"
			: kept > 0
				? ` — ${kept} unsent message(s) kept; Esc on an empty editor restores`
				: "";
		switch (view.status) {
			case "loading":
				this.#status = `Loading oqt — type now; Enter queues one message${suffix}`;
				break;
			case "queued":
				this.#status = "Queued — sends when oqt is ready (Esc cancels)";
				break;
			case "sending":
				this.#status = "Sending queued message…";
				break;
			case "sent":
			case "idle":
				if (
					!this.#startupPending &&
					/^(Loading oqt|Queued —|Sending queued)/.test(this.#status)
				)
					this.#status = "";
				break;
			case "uncertain":
				this.#status = `Queued message may have been sent (connection lost): check the transcript before resending${suffix}`;
				break;
			default:
				this.#status = `${this.#startupNotice || "Queued message not sent"}${suffix}`;
		}
		this.#rebuild();
	}

	#queueStartupInput(text: string): void {
		const startup = this.#startup;
		const trimmed = text.trim();
		if (trimmed.length === 0) return;
		if (trimmed.startsWith("/") || trimmed.startsWith("!")) {
			this.#chatInput.setText(text);
			this.#status = "Commands are available once oqt is ready — draft kept";
			this.#rebuild();
			return;
		}
		if (startup?.input.submit(text)) return;
		this.#chatInput.setText(text);
		const phase = startup?.input.view();
		this.#status =
			phase?.phase === "failed"
				? "oqt did not start — draft kept; exit and relaunch"
				: phase?.queued || phase?.sending
					? "A startup message is already queued — draft kept (Esc cancels the queued one)"
					: "Not queued — draft kept";
		this.#rebuild();
	}

	// ─── Component ───────────────────────────────────────────────────────────

	get layoutRoot(): Component {
		return this.#layoutRoot;
	}

	render(width: number): string[] {
		return [
			...this.#documentContainer.render(width),
			...this.#pendingMessagesContainer.render(width),
			...this.#statusContainer.render(width),
			...this.#editorContainer.render(width),
			...this.#footerComponent.render(width),
		];
	}

	handleInput(data: string): void {
		if (process.env.OQT_DEBUG && process.env.OQT_INPUT_TRACE)
			appendFileSync(
				process.env.OQT_INPUT_TRACE,
				`${new Date().toISOString()} busy=${this.#busy} screen=${this.#screen} select=${this.#selectList !== undefined} status=${JSON.stringify(this.#status)} data=${Buffer.from(data).toString("hex")}\n`,
			);
		if (this.#busy && !this.#startupPending && this.#screen === "chat") {
			if (
				this.#keybindings.matches(data, "app.clear") ||
				(this.#chatInput.getText().length === 0 &&
					this.#keybindings.matches(data, "app.exit"))
			)
				this.#finish();
			return;
		}
		if (this.#screen === "chat") {
			this.#chatInput.handleInput(data);
			this.#requestRender();
			return;
		}
		const selection = this.#selection;
		if (
			selection !== undefined &&
			selection.items.length <= 9 &&
			!this.#selectQuery
		) {
			const digit = /^[1-9]$/.test(data) ? Number(data) - 1 : -1;
			const item = digit >= 0 ? selection.items[digit] : undefined;
			if (item !== undefined) {
				this.#completeSelection(item.value);
				return;
			}
		}
		if (selection !== undefined) {
			// Printable keys search (digits pick in short menus); Backspace edits; Esc clears first.
			const printable =
				data.length === 1 && data >= " " && data !== "\u007f"
					? data
					: decodeKittyPrintable(data);
			if (printable !== undefined && printable !== " ") {
				this.#selectQuery += printable;
				this.#rebuild();
				return;
			}
			if ((data === "\u007f" || data === "\b") && this.#selectQuery) {
				this.#selectQuery = this.#selectQuery.slice(0, -1);
				this.#rebuild();
				return;
			}
			if (data === "\u001b" && this.#selectQuery) {
				this.#selectQuery = "";
				this.#rebuild();
				return;
			}
		}
		this.#selectList?.handleInput(data);
	}

	invalidate(): void {
		this.#layoutRoot.invalidate();
	}

	dispose(): void {
		void this.close().catch(() => {});
	}

	refreshTheme(): void {
		const view = this.#view();
		if (view !== undefined) this.#chatView?.refreshTheme(view);
		this.#rebuild();
	}

	showError(error: string): void {
		this.#status = `Error: ${error}`;
		this.#rebuild();
	}

	close(): Promise<void> {
		this.#closePromise ??= this.#close();
		return this.#closePromise;
	}

	async #close(): Promise<void> {
		this.#closed = true;
		this.#startup?.close();
		programStatus.detach();
		this.#completeSelection(undefined);
		await this.#transition.catch(() => {});
		for (const cleanup of this.#pluginCleanups.splice(0)) {
			try {
				cleanup();
			} catch {
				/* best effort */
			}
		}
		this.#detach();
		this.#sessions?.release();
		this.#sessions = undefined;
	}

	#view(): ConversationView | undefined {
		const ref = this.#conversation;
		if (!ref || ref.status !== "live") return this.#chatViewValue;
		try {
			this.#chatViewValue = ref.view.value as ConversationView;
		} catch {
			/* not hydrated yet */
		}
		return this.#chatViewValue;
	}
	#chatViewValue: ConversationView | undefined;

	#rebuild(): void {
		const statusView = this.#view();
		if (statusView !== undefined)
			programStatus.observe(
				liveOf(statusView).run !== undefined,
				statusView.entries as never,
			);
		this.#statusContainer.clear();
		if (this.#status.length > 0)
			this.#statusContainer.addChild(
				new Text(theme.fg("dim", this.#status), 1, 0),
			);
		if (this.#chatView !== undefined)
			this.#statusContainer.addChild(this.#chatView.status);
		this.#footerComponent.setText(theme.fg("dim", this.#footer()));
		this.#editorContainer.clear();
		if (this.#screen === "select" && this.#selection !== undefined) {
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
					? fuzzyFilter(
							all,
							query,
							(item) => `${item.label} ${item.description ?? ""} ${item.value}`,
						)
					: all;
				const hint = searchable
					? theme.fg(
							"dim",
							query
								? `  search: ${query}  (${items.length}/${all.length})`
								: "  type to search",
						)
					: "";
				selector.addChild(
					new Text(
						`${theme.bold(this.#selection.title)}${hint ? `\n${hint}` : ""}`,
						1,
						1,
					),
				);
				this.#selectList = new SelectList(
					items,
					Math.min(Math.max(items.length, 1), 12),
					selectTheme,
				);
				const selectedIndex = items.findIndex(
					(item) => item.value === this.#selection?.selectedValue,
				);
				if (selectedIndex >= 0)
					this.#selectList.setSelectedIndex(selectedIndex);
				this.#selectList.onSelect = (item) =>
					this.#completeSelection(item.value);
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
			this.#selectQuery = "";
			this.#selectList = undefined;
			this.#chatInput.focused = !this.#busy;
			this.#editorContainer.addChild(this.#chatInput);
		}
		this.#layoutRoot.invalidate();
		this.#requestRender();
	}

	#select(
		title: string,
		items: readonly SelectItem[],
		selectedValue?: string,
	): Promise<string | undefined> {
		if (this.#selection !== undefined)
			throw new Error("A selector is already active");
		return new Promise((resolve) => {
			this.#selection = {
				title,
				items: items.map((item) => ({ ...item })),
				...(selectedValue === undefined ? {} : { selectedValue }),
				resolve,
			};
			this.#screen = "select";
			this.#rebuild();
		});
	}

	#completeSelection(value: string | undefined): void {
		const selection = this.#selection;
		if (selection === undefined) return;
		this.#selection = undefined;
		this.#screen = "chat";
		selection.resolve(value);
		if (!this.#closed) this.#rebuild();
	}

	#updateAutocomplete(): void {
		const commands = this.#commands.list();
		this.#chatInput.setAutocompleteProvider(
			new CombinedAutocompleteProvider(
				commands.map((command) => ({
					name: command.name,
					description: command.description,
					...(command.argumentHint === undefined
						? {}
						: { argumentHint: command.argumentHint }),
					...(command.getArgumentCompletions === undefined
						? {}
						: {
								getArgumentCompletions: async (prefix: string) => {
									const items =
										(await command.getArgumentCompletions?.(prefix)) ?? null;
									return items === null ? null : [...items];
								},
							}),
				})),
				process.cwd(),
			),
		);
		this.#requestRender();
	}

	// ─── Input ───────────────────────────────────────────────────────────────

	async #runPrompt(messageText: string): Promise<void> {
		if (this.#startupPending) {
			this.#queueStartupInput(messageText);
			return;
		}
		const prompt = messageText.trim();
		if (prompt.length === 0) return;
		if (prompt.startsWith("!") && this.#sessionId !== undefined) {
			await this.#executeSlashCommand(
				"oqt-shell",
				`${this.#sessionId}\n${prompt}`,
			);
			return;
		}
		if (prompt === "/shell-results" && this.#sessionId !== undefined) {
			await this.#executeSlashCommand("oqt-shell-results", this.#sessionId);
			return;
		}
		if (prompt.startsWith("/")) {
			const separator = prompt.indexOf(" ");
			const name = prompt.slice(1, separator === -1 ? undefined : separator);
			const args = separator === -1 ? "" : prompt.slice(separator + 1).trim();
			await this.#executeSlashCommand(name, args);
			return;
		}
		this.#chatInput.setText("");
		const conversation = this.#conversation;
		if (!conversation) {
			this.#chatInput.setText(messageText);
			this.#status = "No conversation attached — draft kept";
			this.#rebuild();
			return;
		}
		const running =
			this.#view() !== undefined &&
			liveOf(this.#view() as ConversationView).run !== undefined;
		this.#status = running ? "Queueing steering message…" : "Running turn…";
		this.#rebuild();
		const result = await this.#send(
			conversation,
			prompt,
			`prompt-${randomUUID()}`,
			running,
		);
		if (!result.accepted) {
			this.#status = `Message rejected: ${result.error}`;
			if (this.#chatInput.getText().length === 0)
				this.#chatInput.setText(messageText);
		} else this.#status = running ? `Queued ${result.id}.` : "";
		this.#rebuild();
	}

	/** Sends once; if the link dropped mid-call, retries once with the same
	 * request id after reattaching (exactly-once via Durable requestId). */
	async #send(
		conversation: ParticipantRef,
		text: string,
		requestId: string,
		steer = false,
	): Promise<
		{ accepted: true; id: string } | { accepted: false; error: string }
	> {
		const call = (ref: ParticipantRef) =>
			steer ? ref.steer({ text, requestId }) : ref.prompt({ text, requestId });
		try {
			return await call(conversation);
		} catch (error) {
			if (!linkLost(error)) return { accepted: false, error: message(error) };
			await this.#transition;
			const again = this.#conversation;
			if (!again)
				return {
					accepted: false,
					error: "Connection lost; the message may not have been sent",
				};
			try {
				return await call(again);
			} catch (retry) {
				return {
					accepted: false,
					error: `Connection lost; the message may not have been sent (${message(retry)})`,
				};
			}
		}
	}

	async #queueFollowUp(text: string): Promise<void> {
		if (this.#startupPending) {
			this.#queueStartupInput(text);
			return;
		}
		const conversation = this.#conversation;
		if (!conversation) return;
		try {
			this.#status = "Queueing follow-up…";
			this.#rebuild();
			const result = await conversation.followUp({
				text,
				requestId: `follow-${randomUUID()}`,
			});
			this.#status = result.accepted
				? `Queued ${result.id}.`
				: `Message rejected: ${result.error}`;
		} catch (error) {
			this.#status = `Error: ${message(error)}`;
		}
		this.#rebuild();
	}

	async #executeSlashCommand(name: string, args: string): Promise<void> {
		if (this.#startupPending) {
			this.#status = "Commands are available once oqt is ready";
			this.#rebuild();
			return;
		}
		const command = this.#commands
			.list()
			.find((candidate) => candidate.name === name);
		this.#chatInput.setText("");
		if (command === undefined) {
			this.#status = `Unknown slash command: /${name}`;
			this.#rebuild();
			return;
		}
		try {
			const result: SlashCommandRunResult = await command.run(
				args,
				BACKGROUND_CONTEXT,
			);
			if (result !== undefined) {
				this.#status = result.accepted
					? ""
					: `Operation rejected: ${result.error}`;
				this.#rebuild();
			}
		} catch (error) {
			this.#status = `Error: ${message(error)}`;
			this.#rebuild();
		}
	}

	#interrupt(): void {
		if (this.#startupPending) {
			const startup = this.#startup;
			// cancel() notifies once; that render restores the text and says so.
			if (startup?.input.cancel() !== undefined) return;
			const next = startup?.input.view().retained[0];
			if (
				startup !== undefined &&
				next !== undefined &&
				this.#chatInput.getText().length === 0
			) {
				this.#chatInput.setText(next.text);
				startup.input.release(next.requestId);
			}
			this.#startupChanged();
			return;
		}
		const view = this.#view();
		const conversation = this.#conversation;
		if (
			view === undefined ||
			liveOf(view).run === undefined ||
			conversation === undefined
		)
			return;
		this.#status = "Aborting…";
		this.#rebuild();
		void conversation.abort().then(
			() => {
				if (this.#status !== "Aborting…") return;
				this.#status = "";
				this.#rebuild();
			},
			(error: unknown) => {
				this.#status = `Error: ${message(error)}`;
				this.#rebuild();
			},
		);
	}

	// ─── Built-in commands ──────────────────────────────────────────────────

	#registerBuiltIns(): void {
		const ui = this.#pluginApi().ui;
		const requireConversation = () => {
			const conversation = this.#conversation;
			if (!conversation) throw new Error("No conversation attached");
			return conversation;
		};
		// replace(): a plugin's registration of the same name stays first.
		this.#commands.replace({
			name: "thinking",
			description: "Select reasoning level",
			run: async (_args, context: Context) => {
				const conversation = requireConversation(); // pinned to the conversation shown now
				const levels = await conversation.thinkingLevels();
				const current =
					((this.#view()?.docs["pi.agent"] ?? {}) as AgentState)
						.thinkingLevel ?? "off";
				const picked = await ui.select(
					"Reasoning level",
					levels.map((level) => ({
						value: level,
						label: level,
						description: THINKING_LABELS[level],
					})),
					current,
					context,
				);
				if (picked)
					await conversation.selectThinking({ level: picked as never });
				return undefined;
			},
		});
		this.#commands.replace({
			name: "compact",
			description: "Summarize older messages (optional instructions)",
			argumentHint: "[instructions]",
			run: async (args, context: Context) => {
				const conversation = requireConversation();
				ui.showStatus("Compacting…", context);
				const result = await conversation.compact(
					args ? { instructions: args } : {},
				);
				ui.showStatus(
					result.accepted
						? "Compaction started."
						: `Compaction rejected: ${result.error}`,
					context,
				);
				return undefined;
			},
		});
	}

	#footer(): string {
		const view = this.#view();
		if (!view) return "/model · /thinking · /compact";
		const agent = (view.docs["pi.agent"] ?? {}) as AgentState;
		const model =
			agent.model === undefined
				? "no model"
				: `${agent.model.provider}/${agent.model.modelId}`;
		let inputTokens = 0;
		let outputTokens = 0;
		for (const entry of view.entries)
			for (const item of entry.model ?? [])
				if (item.role === "assistant" && item.usage) {
					inputTokens += item.usage.input ?? 0;
					outputTokens += item.usage.output ?? 0;
				}
		return `${process.cwd()} · ${model} · thinking:${agent.thinkingLevel ?? "off"} · ↑${formatTokens(inputTokens)} ↓${formatTokens(outputTokens)} · /model · /thinking · /compact`;
	}
}
