/**
 * React hook for security prompt system.
 *
 * Connects to /api/prompts/ws to receive real-time prompt requests
 * from oqto-guard and oqto-ssh-proxy.
 */

import {
	getAuthToken,
	getControlPlaneBaseUrl,
} from "@/lib/control-plane-client";
import { useCallback, useEffect, useRef, useState } from "react";

// ============================================================================
// Types
// ============================================================================

export type PromptSource = "octo_guard" | "octo_ssh_proxy" | "network" | string;

export type PromptType =
	| "file_read"
	| "file_write"
	| "ssh_sign"
	| "network_access"
	| string;

export type PromptAction = "allow_once" | "allow_session" | "deny";

export type PromptStatus = "pending" | "responded" | "timed_out" | "cancelled";

export interface Prompt {
	id: string;
	source: PromptSource;
	prompt_type: PromptType;
	resource: string;
	description?: string;
	context?: unknown;
	status: PromptStatus;
	created_at: string;
	expires_at: string;
	workspace_id?: string;
	session_id?: string;
	response?: {
		action: PromptAction;
		responded_at: string;
	};
}

export type PromptMessage =
	| { type: "created"; prompt: Prompt }
	| { type: "responded"; prompt_id: string; action: PromptAction }
	| { type: "timed_out"; prompt_id: string }
	| { type: "cancelled"; prompt_id: string }
	| { type: "sync"; prompts: Prompt[] };

// ============================================================================
// Hook
// ============================================================================

export interface UsePromptsOptions {
	/** Auto-connect on mount (default: true) */
	autoConnect?: boolean;
}

export interface UsePromptsReturn {
	/** List of pending prompts */
	prompts: Prompt[];

	/** Number of pending prompts */
	pendingCount: number;

	/** Whether connected to WebSocket */
	isConnected: boolean;

	/** Connection error if any */
	error: string | null;

	/** Respond to a prompt */
	respond: (promptId: string, action: PromptAction) => Promise<void>;

	/** Manually connect */
	connect: () => void;

	/** Manually disconnect */
	disconnect: () => void;
}

export function usePrompts(options: UsePromptsOptions = {}): UsePromptsReturn {
	const { autoConnect = true } = options;

	const [prompts, setPrompts] = useState<Prompt[]>([]);
	const [isConnected, setIsConnected] = useState(false);
	const [error, setError] = useState<string | null>(null);

	const wsRef = useRef<WebSocket | null>(null);
	const reconnectTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(
		null,
	);
	const reconnectAttemptRef = useRef(0);

	// Calculate pending count
	const pendingCount = prompts.filter((p) => p.status === "pending").length;

	// Connect to WebSocket
	const connect = useCallback(() => {
		if (wsRef.current?.readyState === WebSocket.OPEN) {
			return;
		}

		const token = getAuthToken();
		if (!token) {
			setError("Not authenticated");
			return;
		}

		const baseUrl = getControlPlaneBaseUrl();
		const wsUrl = `${baseUrl.replace(/^http/, "ws")}/api/prompts/ws`;

		try {
			const ws = new WebSocket(wsUrl, ["oqto-prompts", token]);
			wsRef.current = ws;

			ws.onopen = () => {
				console.log("[prompts] WebSocket connected");
				setIsConnected(true);
				setError(null);
				reconnectAttemptRef.current = 0;
			};

			ws.onmessage = (event) => {
				try {
					const msg: PromptMessage = JSON.parse(event.data);

					switch (msg.type) {
						case "sync":
							// Initial sync of all pending prompts
							setPrompts(msg.prompts.filter((p) => p.status === "pending"));
							break;

						case "created":
							// New prompt created
							setPrompts((prev) => [...prev, msg.prompt]);
							break;

						case "responded":
							// Prompt was responded to
							setPrompts((prev) => prev.filter((p) => p.id !== msg.prompt_id));
							break;

						case "timed_out":
							// Prompt timed out
							setPrompts((prev) => prev.filter((p) => p.id !== msg.prompt_id));
							break;

						case "cancelled":
							// Prompt cancelled
							setPrompts((prev) => prev.filter((p) => p.id !== msg.prompt_id));
							break;
					}
				} catch (e) {
					console.error("[prompts] Failed to parse message:", e);
				}
			};

			ws.onerror = (event) => {
				console.error("[prompts] WebSocket error:", event);
				setError("Connection error");
			};

			ws.onclose = (event) => {
				console.log("[prompts] WebSocket closed:", event.code, event.reason);
				setIsConnected(false);
				wsRef.current = null;

				// Attempt reconnect with exponential backoff
				if (event.code !== 1000) {
					// Not a clean close
					const delay = Math.min(
						1000 * 2 ** reconnectAttemptRef.current,
						30000,
					);
					reconnectAttemptRef.current++;

					console.log(
						`[prompts] Reconnecting in ${delay}ms (attempt ${reconnectAttemptRef.current})`,
					);
					reconnectTimeoutRef.current = setTimeout(connect, delay);
				}
			};
		} catch (e) {
			console.error("[prompts] Failed to create WebSocket:", e);
			setError("Failed to connect");
		}
	}, []);

	// Disconnect from WebSocket
	const disconnect = useCallback(() => {
		if (reconnectTimeoutRef.current) {
			clearTimeout(reconnectTimeoutRef.current);
			reconnectTimeoutRef.current = null;
		}

		if (wsRef.current) {
			wsRef.current.close(1000, "User disconnect");
			wsRef.current = null;
		}

		setIsConnected(false);
	}, []);

	// Respond to a prompt
	const respond = useCallback(
		async (promptId: string, action: PromptAction) => {
			// Can respond via WebSocket or REST
			if (wsRef.current?.readyState === WebSocket.OPEN) {
				wsRef.current.send(
					JSON.stringify({
						type: "respond",
						prompt_id: promptId,
						action,
					}),
				);
			} else {
				// Fallback to REST
				const token = getAuthToken();
				const baseUrl = getControlPlaneBaseUrl();

				const response = await fetch(`${baseUrl}/api/prompts/${promptId}`, {
					method: "POST",
					headers: {
						"Content-Type": "application/json",
						Authorization: `Bearer ${token}`,
					},
					body: JSON.stringify({ action }),
				});

				if (!response.ok) {
					throw new Error(`Failed to respond: ${response.statusText}`);
				}
			}

			// Optimistically remove from list
			setPrompts((prev) => prev.filter((p) => p.id !== promptId));
		},
		[],
	);

	// Auto-connect on mount
	useEffect(() => {
		if (autoConnect) {
			connect();
		}

		return () => {
			disconnect();
		};
	}, [autoConnect, connect, disconnect]);

	return {
		prompts,
		pendingCount,
		isConnected,
		error,
		respond,
		connect,
		disconnect,
	};
}

// ============================================================================
// Helper to get human-readable prompt info
// ============================================================================

export function getPromptTitle(prompt: Prompt): string {
	switch (prompt.source) {
		case "octo_guard":
			return "File Access Request";
		case "octo_ssh_proxy":
			return "SSH Access Request";
		case "network":
			return "Network Access Request";
		default:
			return "Access Request";
	}
}

export function getPromptIcon(prompt: Prompt): string {
	switch (prompt.source) {
		case "octo_guard":
			return "📁";
		case "octo_ssh_proxy":
			return "🔑";
		case "network":
			return "🌐";
		default:
			return "🔒";
	}
}

export function getRemainingTime(prompt: Prompt): number {
	const expiresAt = new Date(prompt.expires_at).getTime();
	const now = Date.now();
	return Math.max(0, Math.floor((expiresAt - now) / 1000));
}
