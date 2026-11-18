import {
	DictationMicButton,
	DictationStatus,
} from "@/components/voice/ComposerDictation";
import type { UseDictationReturn } from "@/features/voice/hooks/useDictation";
import { initI18n } from "@/lib/i18n";
import { DEFAULT_RECOGNITION_SETTINGS } from "@/lib/voice/recognition-settings";
import { WakeMeter } from "@/lib/voice/wake-meter";
import { useState } from "react";
import { createRoot } from "react-dom/client";
import "../../src/styles/globals.css";
import "../../src/oqto-ui/app/shell.css";

initI18n();
const noop = () => {};
const states = [
	"idle",
	"download",
	"unknown",
	"initialize",
	"listening",
	"finishing",
	"error",
] as const;
type State = (typeof states)[number];
function Fixture() {
	const [state, setState] = useState<State>("idle");
	const [draft, setDraft] = useState("The editable draft stays right here.");
	const dictation: UseDictationReturn = {
		isActive: state !== "idle" && state !== "error",
		isConnected: state === "listening",
		getInputVolume: () => 0.15,
		liveTranscript:
			state === "listening"
				? "These words are still being recognized — they are not sent or saved yet."
				: "",
		vadProgress: 0,
		preparation:
			state === "download" || state === "unknown" || state === "initialize"
				? "Downloading speech model…"
				: state === "finishing"
					? "Finishing speech recognition…"
					: null,
		download:
			state === "download"
				? { loaded: 38e6, total: 59e6 }
				: state === "unknown"
					? { loaded: 38e6 }
					: state === "initialize"
						? { loaded: 59e6, total: 59e6 }
						: null,
		error:
			state === "error"
				? "Model download failed. Check your connection and retry."
				: null,
		dismissError: () => setState("idle"),
		recognition: { ...DEFAULT_RECOGNITION_SETTINGS },
		remoteAvailable: false,
		setRecognitionProvider: noop,
		microphones: [],
		refreshMicrophones: async () => {},
		setWakeSilenceTimeoutMs: noop,
		setWakeSound: noop,
		setSendSound: noop,
		sendCountdown: null,
		voiceCue: null,
		setMicrophone: noop,
		setRecognitionLanguage: noop,
		autoSendEnabled: false,
		autoSendPauseMs: 2000,
		setAutoSendEnabled: noop,
		setAutoSendPauseMs: noop,
		start: async () => setState("download"),
		stop: () => setState("finishing"),
		cancel: () => setState("idle"),
		wakeAvailable: false,
		wakeState: "off",
		wakePhrases: [],
		setWakeEnabled: noop,
		setWakePhrase: noop,
		setWakeDebug: noop,
		setWakeThreshold: noop,
		wakeReleaseThreshold: null,
		wakeMeter: new WakeMeter(),
		wakeTriggerScore: null,
		setWakePlayback: noop,
	};
	return (
		<main
			className="wb-shell"
			style={{ padding: "24px", display: "block", height: "100vh" }}
		>
			<nav style={{ display: "flex", gap: "8px", flexWrap: "wrap" }}>
				{states.map((value) => (
					<button
						type="button"
						key={value}
						data-fixture-state={value}
						onClick={() => setState(value)}
					>
						{value}
					</button>
				))}
			</nav>
			<section data-fixture-chat style={{ height: "55vh", padding: "24px 0" }}>
				<p>A quiet composer, with ears.</p>
				<p>Chat stays still while speech prepares, listens and finishes.</p>
			</section>
			<footer
				className="wb-composer composer-dictation-anchor"
				data-fixture-composer
			>
				<DictationStatus dictation={dictation} />
				<DictationMicButton dictation={dictation} />
				<textarea
					aria-label="Draft"
					rows={1}
					value={draft}
					onChange={(event) => setDraft(event.target.value)}
				/>
				<button type="button" className="wb-icon-button" aria-label="Send">
					↗
				</button>
			</footer>
		</main>
	);
}
const root = document.getElementById("root");
if (!root) throw new Error("Fixture root missing");
createRoot(root).render(<Fixture />);
