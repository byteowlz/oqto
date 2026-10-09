import type { UseDictationReturn } from "@/features/voice/hooks/useDictation";
import type {
	STTCallbacks,
	STTRecognitionSettings,
} from "@/lib/voice/stt-service";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ChatPane } from "../src/oqto-ui/chat/ChatPane";
import { scriptedOqtoUiPlatform } from "../src/oqto-ui/dev/scripted-platform";

// Presentation is owned/tested separately. Keep the real hook and mock only
// capture: these controls exercise the exact callbacks passed by ChatPane.
vi.mock("@/components/voice/ComposerDictation", () => ({
	DictationMicButton: ({ dictation }: { dictation: UseDictationReturn }) => (
		<>
			<button type="button" onClick={() => void dictation.start()}>
				Start dictation
			</button>
			<button
				type="button"
				onClick={() => dictation.setAutoSendEnabled(!dictation.autoSendEnabled)}
			>
				Auto-send {dictation.autoSendEnabled ? "on" : "off"}
			</button>
			<span>
				{dictation.remoteAvailable ? "Remote available" : "Remote unavailable"}
			</span>
		</>
	),
	DictationStatus: ({ dictation }: { dictation: UseDictationReturn }) =>
		dictation.isActive ? (
			<div>
				<span>{dictation.liveTranscript}</span>
				<button type="button" onClick={dictation.stop}>
					Finish
				</button>
				<button type="button" onClick={dictation.cancel}>
					Cancel dictation
				</button>
			</div>
		) : null,
}));

vi.mock("react-i18next", () => ({
	useTranslation: () => ({
		t: (key: string) => key,
		i18n: { resolvedLanguage: "en" },
	}),
}));

const captures: Capture[] = [];
class Capture {
	callbacks: STTCallbacks = {};
	listening = false;
	constructor(
		public url: string,
		_timeout: number,
		public selection: STTRecognitionSettings,
	) {
		captures.push(this);
	}
	setCallbacks(callbacks: STTCallbacks) {
		this.callbacks = callbacks;
	}
	async startListening() {
		this.listening = true;
		this.callbacks.onConnectionChange?.(true);
	}
	async finishListening() {
		this.callbacks.onFinal?.("drained final");
	}
	disconnect() {
		this.listening = false;
	}
	getInputVolume() {
		return 0;
	}
}
vi.mock("@/lib/voice/stt-service", () => ({
	STTService: vi
		.fn()
		.mockImplementation(
			(...args: ConstructorParameters<typeof Capture>) => new Capture(...args),
		),
}));
vi.mock("@/lib/control-plane-client", () => ({
	voiceProxyWsUrl: vi.fn(() => "wss://never-used.test"),
}));

const draftKey = (platform: string, workspace: string, session: string) =>
	`oqto-ui:chat-draft:${JSON.stringify([platform, workspace, session])}`;
let storage: Map<string, string>;
beforeEach(() => {
	captures.length = 0;
	storage = new Map();
	vi.mocked(localStorage.getItem).mockImplementation(
		(key) => storage.get(key) ?? null,
	);
	vi.mocked(localStorage.setItem).mockImplementation((key, value) => {
		storage.set(key, value);
	});
	vi.mocked(localStorage.removeItem).mockImplementation((key) => {
		storage.delete(key);
	});
	vi.useRealTimers();
});

function mount(
	sessionId = "A",
	workspacePath = "/workspace",
	platformId = "test",
) {
	const send = vi.fn(() => "client-prompt");
	const abort = vi.fn();
	const bind = vi.fn(() => vi.fn());
	const platform = {
		...scriptedOqtoUiPlatform,
		id: platformId,
		chat: { ...scriptedOqtoUiPlatform.chat, send, abort, bind },
	};
	const client = new QueryClient({
		defaultOptions: { queries: { retry: false } },
	});
	const view = (session: string, workspace: string, id: string) => (
		<QueryClientProvider client={client}>
			<ChatPane
				platform={{ ...platform, id }}
				agentName="Pi"
				sessionId={session}
				workspacePath={workspace}
				tasks={[]}
				onOpenFile={vi.fn()}
			/>
		</QueryClientProvider>
	);
	const result = render(view(sessionId, workspacePath, platformId));
	return {
		...result,
		send,
		abort,
		bind,
		switchScope: (
			session: string,
			workspace = workspacePath,
			id = platformId,
		) => result.rerender(view(session, workspace, id)),
	};
}
async function start() {
	await act(async () => {
		fireEvent.click(screen.getByRole("button", { name: "Start dictation" }));
	});
	return captures[captures.length - 1] as Capture;
}
function type(value: string) {
	fireEvent.change(screen.getByRole("textbox"), { target: { value } });
}

describe("ChatPane scoped dictation wiring", () => {
	it("starts locally without remote configuration, keeps textarea and previews out of persisted drafts", async () => {
		const { send } = mount();
		type("typed draft");
		const textarea = screen.getByRole("textbox");
		const capture = await start();
		expect(capture.url).toBe("");
		expect(capture.selection.provider).toBe("moonshine");
		expect(screen.getByText("Remote unavailable")).toBeInTheDocument();
		expect(
			screen.getByRole("button", { name: "Auto-send off" }),
		).toBeInTheDocument();
		act(() => capture.callbacks.onPreview?.("provisional words"));
		expect(screen.getByText("provisional words")).toBeInTheDocument();
		expect(screen.getByRole("textbox")).toBe(textarea);
		expect(textarea).toHaveValue("typed draft");
		expect(storage.get(draftKey("test", "/workspace", "A"))).toBe(
			"typed draft",
		);
		fireEvent.click(screen.getByRole("button", { name: "Cancel dictation" }));
		expect(textarea).toHaveValue("typed draft");
		expect(send).not.toHaveBeenCalled();
	});

	it("persists completed speech and typed edits across remount, then clears only after existing send", async () => {
		const first = mount();
		type("typed");
		const capture = await start();
		act(() => capture.callbacks.onFinal?.("completed"));
		expect(screen.getByRole("textbox")).toHaveValue("typed completed");
		expect(storage.get(draftKey("test", "/workspace", "A"))).toBe(
			"typed completed",
		);
		first.unmount();
		const second = mount();
		expect(screen.getByRole("textbox")).toHaveValue("typed completed");
		fireEvent.click(screen.getByRole("button", { name: "oqtoUi.chat.send" }));
		expect(second.send).toHaveBeenCalledExactlyOnceWith(
			"A",
			"typed completed",
			"steer",
		);
		expect(storage.has(draftKey("test", "/workspace", "A"))).toBe(false);
		expect(screen.getByRole("textbox")).toHaveValue("");
		fireEvent.click(screen.getByRole("button", { name: "oqtoUi.chat.abort" }));
		expect(second.abort).toHaveBeenCalledExactlyOnceWith("A");
	});

	it("Finish drains completion without sending even with opt-in enabled", async () => {
		const { send } = mount();
		type("keep");
		fireEvent.click(screen.getByRole("button", { name: "Auto-send off" }));
		const capture = await start();
		act(() => capture.callbacks.onPreview?.("not committed"));
		await act(async () => {
			fireEvent.click(screen.getByRole("button", { name: "Finish" }));
		});
		expect(screen.getByRole("textbox")).toHaveValue("keep drained final");
		expect(storage.get(draftKey("test", "/workspace", "A"))).toBe(
			"keep drained final",
		);
		expect(send).not.toHaveBeenCalled();
	});

	it.each([
		["B", "/workspace", "test"],
		["A", "/other", "test"],
		["A", "/workspace", "other-platform"],
	])(
		"cancels and restores independently after identity changes to %s/%s/%s",
		async (session, workspace, id) => {
			const pane = mount();
			type("A retained");
			fireEvent.click(screen.getByRole("button", { name: "Auto-send off" }));
			const old = await start();
			vi.useFakeTimers();
			act(() => old.callbacks.onFinal?.("A complete"));
			storage.set(draftKey(id, workspace, session), "other retained");
			pane.switchScope(session, workspace, id);
			expect(old.listening).toBe(false);
			expect(screen.getByRole("textbox")).toHaveValue("other retained");
			act(() => {
				old.callbacks.onFinal?.("stale final");
				old.callbacks.onPreview?.("stale preview");
				vi.runOnlyPendingTimers();
			});
			expect(pane.send).not.toHaveBeenCalled();
			expect(storage.get(draftKey(id, workspace, session))).toBe(
				"other retained",
			);
			pane.switchScope("A", "/workspace", "test");
			expect(screen.getByRole("textbox")).toHaveValue("A retained A complete");
			vi.useRealTimers();
		},
	);

	it("fences a Finish still draining during a scope replacement", async () => {
		const pane = mount();
		type("original draft");
		const old = await start();
		let finish: () => void = () => {};
		vi.spyOn(old, "finishListening").mockImplementation(
			() =>
				new Promise<void>((resolve) => {
					finish = resolve;
				}),
		);
		fireEvent.click(screen.getByRole("button", { name: "Finish" }));
		pane.switchScope("B");
		type("new draft");
		const current = await start();
		await act(async () => {
			old.callbacks.onFinal?.("old drained completion");
			finish();
			await Promise.resolve();
		});
		expect(current.listening).toBe(true);
		expect(screen.getByRole("textbox")).toHaveValue("new draft");
		expect(storage.get(draftKey("test", "/workspace", "A"))).toBe(
			"original draft",
		);
		expect(pane.send).not.toHaveBeenCalled();
	});

	it("auto-send is opt-in and uses the current composer draft through platform send", async () => {
		const pane = mount();
		type("typed");
		const capture = await start();
		vi.useFakeTimers();
		act(() => {
			capture.callbacks.onFinal?.("first");
			vi.runOnlyPendingTimers();
		});
		expect(pane.send).not.toHaveBeenCalled();
		fireEvent.click(screen.getByRole("button", { name: "Auto-send off" }));
		act(() => capture.callbacks.onFinal?.("second"));
		act(() => vi.runOnlyPendingTimers());
		expect(pane.send).toHaveBeenCalledExactlyOnceWith(
			"A",
			"typed first second",
			"steer",
		);
		expect(storage.has(draftKey("test", "/workspace", "A"))).toBe(false);
		vi.useRealTimers();
	});
});
