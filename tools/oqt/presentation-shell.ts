/** First-frame startup screen: the native Pi editor component and keybindings,
 * nothing from the client/server stack. It owns the editor only until the full
 * TUI takes over (presentation-client.ts), which adopts this draft and the same
 * startup queue in one synchronous step. It never dispatches anything itself.
 */
import {
	type Component,
	Container,
	setKeybindings,
	Text,
	type TUI,
} from "@earendil-works/pi-tui";
import { KeybindingsManager } from "../core/keybindings.ts";
import { createChatViewport } from "../modes/interactive/chat-viewport.ts";
import { CustomEditor } from "../modes/interactive/components/custom-editor.ts";
import { getEditorTheme, theme } from "../modes/interactive/theme/theme.ts";
import type { createPresentationStartup } from "./presentation-startup.ts";

type StartupQueue = ReturnType<typeof createPresentationStartup>;

export class StartupShell implements Component {
	readonly editor: CustomEditor;
	readonly layoutRoot: Component;
	readonly #status = new Text("", 1, 0);
	readonly #statusContainer = new Container();
	readonly #editorContainer = new Container();
	readonly #queue: StartupQueue;
	readonly #requestRender: () => void;
	#notice = "";

	constructor(ui: TUI, queue: StartupQueue, finish: () => void) {
		this.#queue = queue;
		this.#requestRender = () => ui.requestRender();
		const keybindings = KeybindingsManager.create();
		setKeybindings(keybindings);
		this.editor = new CustomEditor(ui, getEditorTheme(), keybindings, {
			paddingX: 1,
		});
		this.editor.onSubmit = (text) => this.#submit(text);
		this.editor.onEscape = () => this.#cancel();
		this.editor.onCtrlD = finish;
		this.editor.onAction("app.clear", finish);
		this.#editorContainer.addChild(this.editor);
		this.#statusContainer.addChild(this.#status);
		this.layoutRoot = createChatViewport({
			document: new Container(),
			pendingMessages: new Container(),
			status: this.#statusContainer,
			editor: this.#editorContainer,
			footer: new Text("", 1, 0),
			scrollbarTrackStyle: (text) => theme.fg("scrollbarTrack", text),
			scrollbarThumbStyle: (text) => theme.fg("scrollbarThumb", text),
		}).root;
		this.refresh();
	}

	render(width: number): string[] {
		return [
			...this.#statusContainer.render(width),
			...this.#editorContainer.render(width),
		];
	}

	invalidate(): void {
		this.layoutRoot.invalidate();
	}

	/** Re-render from queue state; restores withdrawn/failed text to an empty editor. */
	refresh(): void {
		const view = this.#queue.input.view();
		let restored = false;
		for (const item of view.retained) {
			if (this.editor.getText().length > 0) break;
			this.editor.setText(item.text);
			this.#queue.input.release(item.requestId);
			restored = true;
		}
		const suffix = restored ? " — text restored to the editor" : "";
		const text =
			this.#notice ||
			(view.queued
				? "Queued — sends when oqt is ready (Esc cancels)"
				: `Loading oqt — type now; Enter queues one message${suffix}`);
		this.#status.setText(theme.fg("dim", text));
		this.#requestRender();
	}

	/** Startup failed before the full TUI loaded: keep text, send nothing. */
	fail(reason: string): void {
		this.#notice = `Startup failed; nothing was sent: ${reason}`;
		this.#queue.fail();
		this.refresh();
	}

	#submit(text: string): void {
		const trimmed = text.trim();
		if (!trimmed) return;
		if (trimmed.startsWith("/")) {
			this.editor.setText(text);
			this.#status.setText(
				theme.fg(
					"dim",
					"Commands are available once oqt is ready — draft kept",
				),
			);
			this.#requestRender();
			return;
		}
		if (!this.#queue.input.submit(text)) {
			this.editor.setText(text);
			this.#status.setText(
				theme.fg(
					"dim",
					this.#queue.input.view().phase === "failed"
						? "oqt did not start — draft kept; exit and relaunch"
						: "A startup message is already queued — draft kept (Esc cancels the queued one)",
				),
			);
			this.#requestRender();
		}
	}

	#cancel(): void {
		// cancel() notifies the relay, which re-renders this shell and restores text.
		if (this.#queue.input.cancel() !== undefined) return;
		this.refresh();
	}
}
