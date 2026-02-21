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

/**
 * SECURITY: Ensure localStorage belongs to the currently authenticated user.
 *
 * On every page load, decode the JWT from localStorage and compare the user
 * ID against the stored `oqto:currentUserId`. If they don't match, nuke
 * ALL localStorage except the auth token itself. This catches stale data
 * from a previous user even when old cached JavaScript is running.
 */
function enforceUserIsolation() {
	if (typeof window === "undefined") return;
	try {
		const token = localStorage.getItem("oqto:authToken");
		if (!token) return; // Not logged in, nothing to leak

		// Decode JWT payload (middle segment, base64url)
		const parts = token.split(".");
		if (parts.length < 2) return;
		const payload = parts[1].replace(/-/g, "+").replace(/_/g, "/");
		const decoded = JSON.parse(atob(payload));
		const jwtUserId = decoded.sub || decoded.preferred_username;
		if (!jwtUserId) return;

		const storedUserId = localStorage.getItem("oqto:currentUserId");
		if (storedUserId === jwtUserId) return; // Same user, all good

		// User mismatch -- clear everything except token, set correct user,
		// and RELOAD so React starts fresh with no stale state in memory.
		const keysToRemove: string[] = [];
		for (let i = 0; i < localStorage.length; i++) {
			const key = localStorage.key(i);
			if (key && key !== "oqto:authToken" && key !== "oqto:currentUserId") {
				keysToRemove.push(key);
			}
		}
		for (const key of keysToRemove) {
			localStorage.removeItem(key);
		}
		localStorage.setItem("oqto:currentUserId", jwtUserId);
		// Force full reload so React components don't use stale in-memory state
		window.location.reload();
		return; // unreachable, but satisfies control flow
	} catch {
		// If JWT decode fails, clear everything to be safe and reload
		const token = localStorage.getItem("oqto:authToken");
		localStorage.clear();
		if (token) localStorage.setItem("oqto:authToken", token);
		window.location.reload();
	}
}

setupStorageRecovery();
enforceUserIsolation();
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
