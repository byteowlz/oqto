"use client";

import { getAuthToken } from "@/lib/control-plane-client";

function isTerminalDebugEnabled(): boolean {
	if (!import.meta.env.DEV) return false;
	try {
		return localStorage.getItem("debug:terminal") === "1";
	} catch {
		return false;
	}
}
import { FitAddon, Terminal, init } from "ghostty-web";
import {
	forwardRef,
	useCallback,
	useEffect,
	useImperativeHandle,
	useMemo,
	useRef,
	useState,
} from "react";

type SessionConnection = {
	socket: WebSocket | null;
	terminal: Terminal | null;
	fitAddon: FitAddon | null;
	isConnecting: boolean;
	reconnectTimeout: ReturnType<typeof setTimeout> | null;
	reconnectAttempts: number;
};

// Global singleton per workspace session (container)
// Key: sessionId, Value: { socket, terminal, fitAddon, isConnecting }
// Note: ghostty-web terminals cannot be re-opened once opened; they must be disposed on unmount.
// We still keep this map to avoid reconnect storms when props churn, but we do clean up on unmount.
const sessionConnections: Map<string, SessionConnection> = new Map();

function getOrCreateSession(sessionId: string) {
	const existing = sessionConnections.get(sessionId);
	if (existing) return existing;

	const created: SessionConnection = {
		socket: null,
		terminal: null,
		fitAddon: null,
		isConnecting: false,
		reconnectTimeout: null,
		reconnectAttempts: 0,
	};
	sessionConnections.set(sessionId, created);
	return created;
}

// Extract a stable session key from wsUrl.
function extractSessionId(wsUrl: string): string {
	try {
		const url = new URL(wsUrl, window.location.origin);
		const workspacePath = url.searchParams.get("workspace_path");
		if (workspacePath) return `workspace:${workspacePath}`;
		const match = url.pathname.match(/\/session\/([^/]+)\//);
		if (match) return match[1];
		return url.pathname || "default";
	} catch {
		const match = wsUrl.match(/\/session\/([^/]+)\//);
		return match ? match[1] : wsUrl || "default";
	}
}

// Track initialization state globally
let ghosttyInitialized = false;
let ghosttyInitPromise: Promise<void> | null = null;

async function ensureGhosttyInit(): Promise<void> {
	if (ghosttyInitialized) return;
	if (!ghosttyInitPromise) {
		ghosttyInitPromise = init().then(() => {
			ghosttyInitialized = true;
		});
	}
	return ghosttyInitPromise;
}

export type GhosttyTerminalHandle = {
	focus: () => void;
	blur: () => void;
	/** Send a key sequence to the terminal (e.g., for mobile keyboard toolbar) */
	sendKey: (key: string) => void;
};

interface GhosttyTerminalProps {
	wsUrl: string;
	fontFamily?: string;
	fontSize?: number;
	className?: string;
	theme?: string;
}

export const GhosttyTerminal = forwardRef<
	GhosttyTerminalHandle,
	GhosttyTerminalProps
>(
	(
		{
			wsUrl,
			fontFamily = "JetBrainsMono Nerd Font",
			fontSize = 14,
			className,
			theme: themeProp,
		},
		ref,
	) => {
		const containerRef = useRef<HTMLDivElement | null>(null);
		// Start with "connecting" if we have a wsUrl, "waiting" otherwise
		const [status, setStatus] = useState<
			"waiting" | "connecting" | "connected" | "error"
		>(() => (wsUrl ? "connecting" : "waiting"));
		const mountedRef = useRef(true);

		const wsUrlRef = useRef(wsUrl);
		const fontFamilyRef = useRef(fontFamily);
		const fontSizeRef = useRef(fontSize);

		useEffect(() => {
			wsUrlRef.current = wsUrl;
			fontFamilyRef.current = fontFamily;
			fontSizeRef.current = fontSize;
		}, [wsUrl, fontFamily, fontSize]);

		// Extract sessionId from wsUrl and include theme - memoize to avoid recalculation
		// Including theme ensures terminal is recreated with correct colors on theme change
		const sessionId = useMemo(
			() => `${extractSessionId(wsUrl)}-${themeProp || "default"}`,
			[wsUrl, themeProp],
		);

		// Get the session state (creates if doesn't exist)
		const getSession = useCallback(
			() => getOrCreateSession(sessionId),
			[sessionId],
		);

		useImperativeHandle(ref, () => ({
			focus: () => getSession().terminal?.focus(),
			blur: () => getSession().terminal?.blur(),
			sendKey: (key: string) => {
				const session = getSession();
				// Send to both terminal (for local echo if needed) and WebSocket
				if (session.socket?.readyState === WebSocket.OPEN) {
					session.socket.send(key);
				}
			},
		}));

		// Stable callback for handling messages
		const handleMessage = useCallback(
			async (event: MessageEvent) => {
				const session = getSession();
				if (!session.terminal) return;
				if (typeof event.data === "string") {
					session.terminal.write(event.data);
				} else if (event.data instanceof ArrayBuffer) {
					session.terminal.write(new Uint8Array(event.data));
				} else if (event.data instanceof Blob) {
					const buffer = await event.data.arrayBuffer();
					session.terminal.write(new Uint8Array(buffer));
				}
			},
			[getSession],
		);

		useEffect(() => {
			mountedRef.current = true;
			const session = getSession();

			const clearReconnect = () => {
				if (session.reconnectTimeout) {
					clearTimeout(session.reconnectTimeout);
					session.reconnectTimeout = null;
				}
			};

			const scheduleReconnect = (why: string) => {
				if (!mountedRef.current) return;
				const currentWsUrl = wsUrlRef.current;
				if (!currentWsUrl) return;

				clearReconnect();
				session.reconnectAttempts += 1;
				const baseDelay = Math.min(
					10_000,
					250 * 2 ** Math.min(session.reconnectAttempts, 6),
				);
				const jitter = Math.floor(Math.random() * 200);
				const delay = baseDelay + jitter;
				if (isTerminalDebugEnabled()) {
					console.debug(
						`Terminal [${sessionId}]: reconnecting in ${delay}ms (${why})`,
					);
				}
				session.reconnectTimeout = setTimeout(() => {
					session.reconnectTimeout = null;
					void setup();
				}, delay);
			};

			async function setup() {
				const currentWsUrl = wsUrlRef.current;
				// Check socket state more carefully
				const socketState = session.socket?.readyState;
				const isSocketUsable =
					socketState === WebSocket.CONNECTING ||
					socketState === WebSocket.OPEN;
				const hasAttachedTerminal =
					Boolean(session.terminal?.element) &&
					session.terminal?.element === containerRef.current;

				// Skip if already have a usable socket and attached terminal
				if (isSocketUsable && hasAttachedTerminal) {
					if (isTerminalDebugEnabled()) {
						console.debug(
							`Terminal [${sessionId}]: socket already ${socketState === WebSocket.OPEN ? "open" : "connecting"}, skipping`,
						);
					}
					if (mountedRef.current) {
						if (socketState === WebSocket.OPEN) {
							setStatus("connected");
						} else {
							setStatus("connecting");
						}
					}
					return;
				}

				// Reset isConnecting if socket is not usable (closed or null)
				if (!isSocketUsable) {
					session.isConnecting = false;
				}

				if (!currentWsUrl) {
					if (mountedRef.current) setStatus("waiting");
					if (isTerminalDebugEnabled()) {
						console.debug(`Terminal [${sessionId}]: no wsUrl, waiting...`);
					}
					return;
				}

				// Update status to connecting as we start setup
				if (mountedRef.current) setStatus("connecting");

				// Double-check we're not already setting up
					if (session.isConnecting) {
						if (isTerminalDebugEnabled()) {
							console.debug(
								`Terminal [${sessionId}]: setup already in progress, skipping`,
							);
						}
						return;
					}


				session.isConnecting = true;
				if (isTerminalDebugEnabled()) {
					console.debug(`Terminal [${sessionId}]: starting setup...`);
				}

				try {
					// Initialize ghostty
					await ensureGhosttyInit();
					if (!mountedRef.current) {
						session.isConnecting = false;
						return;
					}

					// ghostty-web terminals cannot be re-opened after unmount; recreate if we're mounting into a new container.
					if (
						session.terminal?.element &&
						containerRef.current &&
						session.terminal.element !== containerRef.current
					) {
						if (isTerminalDebugEnabled()) {
							console.debug(`Terminal [${sessionId}]: detected new container, disposing old terminal and socket`);
						}
						// Close old socket so we create a fresh connection
						if (session.socket) {
							try {
								session.socket.onopen = null;
								session.socket.onmessage = null;
								session.socket.onerror = null;
								session.socket.onclose = null;
								session.socket.close();
							} catch {
								// ignore close errors
							}
							session.socket = null;
						}
						try {
							session.terminal.dispose();
						} catch {
							// ignore dispose errors
						}
						session.terminal = null;
						session.fitAddon = null;
					}

					// Create terminal if not exists
					if (!session.terminal && containerRef.current) {
						if (isTerminalDebugEnabled()) {
							console.debug(`Terminal [${sessionId}]: creating terminal...`);
						}
						// Get theme colors from CSS variables
						const computedStyle = getComputedStyle(document.documentElement);
						const terminalBg =
							computedStyle.getPropertyValue("--terminal-bg").trim() ||
							"#0b0d12";
						const terminalFg =
							computedStyle.getPropertyValue("--terminal-fg").trim() ||
							"#f5f5f5";

						const terminal = new Terminal({
							fontFamily: fontFamilyRef.current,
							fontSize: fontSizeRef.current,
							cursorBlink: true,
							convertEol: true,
							theme: {
								background: terminalBg,
								foreground: terminalFg,
							},
						});
						session.terminal = terminal;

						const fitAddon = new FitAddon();
						session.fitAddon = fitAddon;
						terminal.loadAddon(fitAddon);
						terminal.open(containerRef.current);
						fitAddon.fit();

						terminal.onData((data) => {
							if (session.socket?.readyState === WebSocket.OPEN) {
								session.socket.send(data);
							}
						});

						terminal.onResize(({ cols, rows }) => {
							if (session.socket?.readyState === WebSocket.OPEN) {
								const resizeMsg = JSON.stringify({ columns: cols, rows });
								session.socket.send(resizeMsg);
							}
						});

						if (session.socket?.readyState === WebSocket.OPEN) {
							const { cols, rows } = terminal;
							const resizeMsg = JSON.stringify({ columns: cols, rows });
							session.socket.send(resizeMsg);
						}
					}

					// Connect WebSocket if not connected
					if (
						!session.socket ||
						session.socket.readyState === WebSocket.CLOSED
					) {
						clearReconnect();

						// Add auth token as query parameter for WebSocket auth
						let wsUrlWithAuth = currentWsUrl;
						const token = getAuthToken();
						if (token) {
							const separator = currentWsUrl.includes("?") ? "&" : "?";
							wsUrlWithAuth = `${currentWsUrl}${separator}token=${encodeURIComponent(token)}`;
						}

					if (isTerminalDebugEnabled()) {
						console.debug(
							`Terminal [${sessionId}]: connecting WebSocket to ${currentWsUrl.substring(0, 60)}...`,
						);
					}


						const socket = new WebSocket(wsUrlWithAuth);
						socket.binaryType = "arraybuffer";
						session.socket = socket;
						setStatus("connecting");

					socket.onopen = () => {
						if (isTerminalDebugEnabled()) {
							console.debug(`Terminal [${sessionId}]: connected!`);
						}

							session.isConnecting = false;
							session.reconnectAttempts = 0;
							if (mountedRef.current) {
								setStatus("connected");
							}

							// Fit terminal to container and send size to server
							if (session.fitAddon && session.terminal) {
								session.fitAddon.fit();
								const { cols, rows } = session.terminal;
								const resizeMsg = JSON.stringify({ columns: cols, rows });
								socket.send(resizeMsg);
							}
						};

						socket.onmessage = handleMessage;

						socket.onerror = () => {
							console.error(`Terminal [${sessionId}]: websocket error`);
							session.isConnecting = false;
							if (mountedRef.current) {
								setStatus("error");
							}
							scheduleReconnect("error");
						};

					socket.onclose = (event) => {
						if (isTerminalDebugEnabled()) {
							console.debug(
								`Terminal [${sessionId}]: connection closed (code=${event.code} clean=${event.wasClean})`,
							);
						}

							session.isConnecting = false;
							session.socket = null;
							if (mountedRef.current) {
								setStatus("error");
							}
							scheduleReconnect("close");
						};
					}
				} catch (err) {
					console.error(`Terminal [${sessionId}]: setup error`, err);
					session.isConnecting = false;
					scheduleReconnect("setup error");
				}
			}

			void setup();

			return () => {
				clearReconnect();
			};
		}, [handleMessage, sessionId, getSession]);

		// Cleanup resources on unmount (or when switching sessionId).
		// Use delayed cleanup to handle React Strict Mode double-mounting.
		useEffect(() => {
			const currentSessionId = sessionId;
			// Capture the container element at mount time to detect if a new instance took over
			const myContainer = containerRef.current;

			return () => {
				mountedRef.current = false;

				// Delay cleanup to allow for React Strict Mode remount
				setTimeout(() => {
					const session = sessionConnections.get(currentSessionId);
					if (!session) return;

					// If terminal is now attached to a DIFFERENT container, a new instance took over
					// Don't clean up - the new instance is using this session
					if (session.terminal?.element && session.terminal.element !== myContainer) {
						if (isTerminalDebugEnabled()) {
							console.debug(`Terminal [${currentSessionId}]: skipping cleanup, new instance took over`);
						}
						return;
					}

					// Only cleanup if not remounted (mountedRef would be true if remounted)
					if (mountedRef.current) return;

					if (isTerminalDebugEnabled()) {
						console.debug(`Terminal [${currentSessionId}]: cleaning up resources`);
					}

					if (session.reconnectTimeout) {
						clearTimeout(session.reconnectTimeout);
						session.reconnectTimeout = null;
					}
					if (session.socket) {
						try {
							session.socket.onopen = null;
							session.socket.onmessage = null;
							session.socket.onerror = null;
							session.socket.onclose = null;
							session.socket.close();
						} catch {
							// ignore close errors
						}
						session.socket = null;
					}
					if (session.terminal) {
						try {
							session.terminal.dispose();
						} catch {
							// ignore dispose errors
						}
						session.terminal = null;
					}
					session.fitAddon = null;
					session.isConnecting = false;
					session.reconnectAttempts = 0;

					// Remove from the map to prevent memory leak
					sessionConnections.delete(currentSessionId);
				}, 100); // Small delay to allow strict mode remount
			};
		}, [sessionId]);

		// Handle resize observer separately with throttling
		useEffect(() => {
			if (!containerRef.current) return;
			const session = getSession();

			let resizeTimeout: ReturnType<typeof setTimeout> | null = null;
			let lastResizeTime = 0;
			const THROTTLE_MS = 100; // Throttle to max 10 fit() calls per second

			const handleResize = () => {
				const now = Date.now();
				const timeSinceLastResize = now - lastResizeTime;

				if (resizeTimeout) {
					clearTimeout(resizeTimeout);
					resizeTimeout = null;
				}

				if (timeSinceLastResize >= THROTTLE_MS) {
					// Enough time has passed, fit immediately
					lastResizeTime = now;
					session.fitAddon?.fit();
				} else {
					// Schedule a fit for later
					resizeTimeout = setTimeout(() => {
						lastResizeTime = Date.now();
						session.fitAddon?.fit();
						resizeTimeout = null;
					}, THROTTLE_MS - timeSinceLastResize);
				}
			};

			const observer = new ResizeObserver(handleResize);
			observer.observe(containerRef.current);

			return () => {
				observer.disconnect();
				if (resizeTimeout) {
					clearTimeout(resizeTimeout);
				}
			};
		}, [getSession]);

		return (
			<div
				className={`relative h-full w-full rounded ${className ?? ""}`}
				style={{ backgroundColor: "var(--terminal-bg)", padding: "8px 12px" }}
			>
				<div
					ref={containerRef}
					className="h-full w-full"
					style={{
						// Hide terminal canvas until connected to avoid ghost cursor at 0,0
						visibility: status === "connected" ? "visible" : "hidden",
					}}
				/>
				{/* Only show status indicator when not connected */}
				{status !== "connected" && (
					<div className="absolute inset-0 flex items-center justify-center text-xs font-mono text-muted-foreground">
						{status === "waiting" && "Waiting for session..."}
						{status === "connecting" && "Connecting..."}
						{status === "error" && "Disconnected - retrying..."}
					</div>
				)}
			</div>
		);
	},
);

GhosttyTerminal.displayName = "GhosttyTerminal";
