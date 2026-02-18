// Tauri polyfills MUST be imported first to intercept all fetch/WebSocket calls
import "@/lib/tauri-fetch-polyfill";
import { installTauriWebSocketPolyfill } from "@/lib/tauri-websocket-polyfill";

// Install WebSocket polyfill for Tauri iOS
installTauriWebSocketPolyfill();

import { Providers } from "@/components/providers";
import { initI18n } from "@/lib/i18n";
import React from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App";
import "./styles/globals.css";

const RECOVERY_FLAG_KEY = "oqto:storage_recovery_v1";
function setupStorageRecovery() {
	if (typeof window === "undefined") return;
	if (sessionStorage.getItem(RECOVERY_FLAG_KEY) === "done") return;

	const recover = () => {
		if (sessionStorage.getItem(RECOVERY_FLAG_KEY) === "done") return;
		sessionStorage.setItem(RECOVERY_FLAG_KEY, "done");
		try {
			localStorage.clear();
		} catch {
			// ignore
		}
		location.reload();
	};

	window.addEventListener("error", recover, { once: true });
	window.addEventListener("unhandledrejection", recover, { once: true });
}

setupStorageRecovery();
initI18n();

const container = document.getElementById("root");
if (!container) {
	throw new Error("Root container missing");
}

createRoot(container).render(
	<React.StrictMode>
		<Providers>
			<App />
		</Providers>
	</React.StrictMode>,
);

// Preload removal is handled by AppShell after it's ready to ensure smooth transition
