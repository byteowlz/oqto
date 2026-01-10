/**
 * Tauri WebSocket Polyfill
 *
 * Provides a native WebSocket-compatible interface that uses the Tauri WebSocket plugin
 * on iOS/mobile to bypass WKWebView restrictions on local network connections.
 *
 * Import this once at app startup (e.g., in main.tsx) BEFORE any WebSocket connections.
 */

import { isTauri } from "./tauri-fetch-polyfill";

// Store original WebSocket
const OriginalWebSocket =
	typeof window !== "undefined" ? window.WebSocket : undefined;

// Type for the Tauri WebSocket plugin
type TauriWebSocketMessage =
	| { type: "Text"; data: string }
	| { type: "Binary"; data: number[] }
	| { type: "Ping"; data: number[] }
	| { type: "Pong"; data: number[] }
	| { type: "Close"; data: { code: number; reason: string } | null };

interface TauriWebSocket {
	id: number;
	send(message: string | number[]): Promise<void>;
	disconnect(): Promise<void>;
	addListener(
		callback: (message: TauriWebSocketMessage) => void,
	): Promise<() => void>;
}

// Dynamic import for Tauri WebSocket plugin (only loads in Tauri environment)
let tauriWsConnect: ((url: string) => Promise<TauriWebSocket>) | null = null;

async function getTauriWebSocket() {
	if (tauriWsConnect) return tauriWsConnect;

	try {
		const mod = await import("@tauri-apps/plugin-websocket");
		tauriWsConnect = mod.default?.connect ?? mod.connect;
		return tauriWsConnect;
	} catch (e) {
		console.warn("[TauriWS] Failed to load Tauri WebSocket plugin:", e);
		return null;
	}
}

/**
 * WebSocket wrapper that uses Tauri's native WebSocket plugin on iOS
 * to bypass WKWebView restrictions on local network connections.
 */
class TauriWebSocketPolyfill implements WebSocket {
	// WebSocket constants
	static readonly CONNECTING = 0;
	static readonly OPEN = 1;
	static readonly CLOSING = 2;
	static readonly CLOSED = 3;

	readonly CONNECTING = 0;
	readonly OPEN = 1;
	readonly CLOSING = 2;
	readonly CLOSED = 3;

	// Connection state
	private _readyState = TauriWebSocketPolyfill.CONNECTING;
	private _url: string;
	private _protocol = "";
	private _extensions = "";
	private _bufferedAmount = 0;
	private _binaryType: BinaryType = "blob";

	// Internal Tauri WebSocket
	private _tauriWs: TauriWebSocket | null = null;
	private _unlisten: (() => void) | null = null;

	// Event handlers (property-based)
	onopen: ((this: WebSocket, ev: Event) => unknown) | null = null;
	onmessage: ((this: WebSocket, ev: MessageEvent) => unknown) | null = null;
	onerror: ((this: WebSocket, ev: Event) => unknown) | null = null;
	onclose: ((this: WebSocket, ev: CloseEvent) => unknown) | null = null;

	// Event listeners (addEventListener-based)
	private _listeners: Map<string, Set<EventListenerOrEventListenerObject>> =
		new Map();

	constructor(url: string | URL, protocols?: string | string[]) {
		this._url = typeof url === "string" ? url : url.toString();

		// Store protocol if provided (we don't actually use it with Tauri plugin)
		if (protocols) {
			this._protocol = Array.isArray(protocols)
				? (protocols[0] ?? "")
				: protocols;
		}

		// Initialize connection asynchronously
		this._connect();
	}

	private async _connect(): Promise<void> {
		try {
			const connect = await getTauriWebSocket();
			if (!connect) {
				throw new Error("Tauri WebSocket plugin not available");
			}

			console.debug("[TauriWS] Connecting to:", this._url);
			this._tauriWs = await connect(this._url);
			this._readyState = TauriWebSocketPolyfill.OPEN;

			// Set up message listener
			this._unlisten = await this._tauriWs.addListener((message) => {
				this._handleMessage(message);
			});

			// Fire open event
			const openEvent = new Event("open");
			this._dispatchEvent(openEvent);
			this.onopen?.call(this, openEvent);

			console.debug("[TauriWS] Connected");
		} catch (error) {
			console.error("[TauriWS] Connection failed:", error);
			this._readyState = TauriWebSocketPolyfill.CLOSED;

			const errorEvent = new Event("error");
			this._dispatchEvent(errorEvent);
			this.onerror?.call(this, errorEvent);

			const closeEvent = new CloseEvent("close", {
				code: 1006,
				reason: error instanceof Error ? error.message : "Connection failed",
				wasClean: false,
			});
			this._dispatchEvent(closeEvent);
			this.onclose?.call(this, closeEvent);
		}
	}

	private _handleMessage(message: TauriWebSocketMessage): void {
		switch (message.type) {
			case "Text": {
				const event = new MessageEvent("message", { data: message.data });
				this._dispatchEvent(event);
				this.onmessage?.call(this, event);
				break;
			}
			case "Binary": {
				const data =
					this._binaryType === "arraybuffer"
						? new Uint8Array(message.data).buffer
						: new Blob([new Uint8Array(message.data)]);
				const event = new MessageEvent("message", { data });
				this._dispatchEvent(event);
				this.onmessage?.call(this, event);
				break;
			}
			case "Close": {
				this._readyState = TauriWebSocketPolyfill.CLOSED;
				const closeEvent = new CloseEvent("close", {
					code: message.data?.code ?? 1000,
					reason: message.data?.reason ?? "",
					wasClean: true,
				});
				this._dispatchEvent(closeEvent);
				this.onclose?.call(this, closeEvent);
				break;
			}
			case "Ping":
			case "Pong":
				// Handled by the plugin internally
				break;
		}
	}

	// WebSocket interface implementation
	get readyState(): number {
		return this._readyState;
	}

	get url(): string {
		return this._url;
	}

	get protocol(): string {
		return this._protocol;
	}

	get extensions(): string {
		return this._extensions;
	}

	get bufferedAmount(): number {
		return this._bufferedAmount;
	}

	get binaryType(): BinaryType {
		return this._binaryType;
	}

	set binaryType(value: BinaryType) {
		this._binaryType = value;
	}

	send(data: string | ArrayBufferLike | Blob | ArrayBufferView): void {
		if (this._readyState !== TauriWebSocketPolyfill.OPEN) {
			throw new DOMException("WebSocket is not open", "InvalidStateError");
		}

		if (!this._tauriWs) {
			throw new DOMException("WebSocket not initialized", "InvalidStateError");
		}

		// Convert data to format Tauri expects
		if (typeof data === "string") {
			this._tauriWs.send(data).catch((e) => {
				console.error("[TauriWS] Send failed:", e);
			});
		} else if (data instanceof Blob) {
			data.arrayBuffer().then((buffer) => {
				this._tauriWs?.send(Array.from(new Uint8Array(buffer)));
			});
		} else if (data instanceof ArrayBuffer) {
			this._tauriWs.send(Array.from(new Uint8Array(data))).catch((e) => {
				console.error("[TauriWS] Send failed:", e);
			});
		} else if (ArrayBuffer.isView(data)) {
			this._tauriWs
				.send(
					Array.from(
						new Uint8Array(data.buffer, data.byteOffset, data.byteLength),
					),
				)
				.catch((e) => {
					console.error("[TauriWS] Send failed:", e);
				});
		}
	}

	close(code?: number, reason?: string): void {
		if (
			this._readyState === TauriWebSocketPolyfill.CLOSING ||
			this._readyState === TauriWebSocketPolyfill.CLOSED
		) {
			return;
		}

		this._readyState = TauriWebSocketPolyfill.CLOSING;

		// Clean up listener
		this._unlisten?.();
		this._unlisten = null;

		// Disconnect
		this._tauriWs
			?.disconnect()
			.then(() => {
				this._readyState = TauriWebSocketPolyfill.CLOSED;
				const closeEvent = new CloseEvent("close", {
					code: code ?? 1000,
					reason: reason ?? "",
					wasClean: true,
				});
				this._dispatchEvent(closeEvent);
				this.onclose?.call(this, closeEvent);
			})
			.catch((e) => {
				console.error("[TauriWS] Disconnect failed:", e);
				this._readyState = TauriWebSocketPolyfill.CLOSED;
			});
	}

	// EventTarget implementation
	addEventListener(
		type: string,
		listener: EventListenerOrEventListenerObject | null,
		_options?: boolean | AddEventListenerOptions,
	): void {
		if (!listener) return;
		let listeners = this._listeners.get(type);
		if (!listeners) {
			listeners = new Set();
			this._listeners.set(type, listeners);
		}
		listeners.add(listener);
	}

	removeEventListener(
		type: string,
		listener: EventListenerOrEventListenerObject | null,
		_options?: boolean | EventListenerOptions,
	): void {
		if (!listener) return;
		this._listeners.get(type)?.delete(listener);
	}

	dispatchEvent(event: Event): boolean {
		return this._dispatchEvent(event);
	}

	private _dispatchEvent(event: Event): boolean {
		const listeners = this._listeners.get(event.type);
		if (listeners) {
			for (const listener of listeners) {
				if (typeof listener === "function") {
					listener.call(this, event);
				} else {
					listener.handleEvent(event);
				}
			}
		}
		return !event.defaultPrevented;
	}
}

/**
 * Smart WebSocket constructor that uses native WebSocket on desktop/web
 * and Tauri WebSocket plugin on iOS/mobile.
 */
function createSmartWebSocket(
	url: string | URL,
	protocols?: string | string[],
): WebSocket {
	// Use Tauri WebSocket plugin in Tauri environment
	if (isTauri()) {
		console.debug("[TauriWS] Using Tauri WebSocket plugin for:", url);
		return new TauriWebSocketPolyfill(url, protocols);
	}

	// Use native WebSocket otherwise
	if (!OriginalWebSocket) {
		throw new Error("WebSocket not available");
	}
	return new OriginalWebSocket(url, protocols);
}

// Create a proper WebSocket constructor that extends native WebSocket interface
interface SmartWebSocketConstructor {
	new (url: string | URL, protocols?: string | string[]): WebSocket;
	readonly CONNECTING: 0;
	readonly OPEN: 1;
	readonly CLOSING: 2;
	readonly CLOSED: 3;
	prototype: WebSocket;
}

// Augment the constructor with static properties
const SmartWebSocket =
	createSmartWebSocket as unknown as SmartWebSocketConstructor;
Object.defineProperties(SmartWebSocket, {
	CONNECTING: {
		value: 0,
		writable: false,
		enumerable: true,
		configurable: false,
	},
	OPEN: { value: 1, writable: false, enumerable: true, configurable: false },
	CLOSING: { value: 2, writable: false, enumerable: true, configurable: false },
	CLOSED: { value: 3, writable: false, enumerable: true, configurable: false },
	prototype: {
		value: OriginalWebSocket?.prototype ?? TauriWebSocketPolyfill.prototype,
		writable: false,
	},
});

/**
 * Install the WebSocket polyfill (replaces window.WebSocket)
 */
export function installTauriWebSocketPolyfill(): void {
	if (typeof window === "undefined") return;

	// Only install in Tauri environment
	if (isTauri()) {
		console.debug("[TauriWS] Installing WebSocket polyfill");
		// @ts-expect-error - Replacing global WebSocket
		window.WebSocket = SmartWebSocket;
	}
}

/**
 * Restore original WebSocket
 */
export function restoreWebSocket(): void {
	if (typeof window === "undefined" || !OriginalWebSocket) return;
	window.WebSocket = OriginalWebSocket;
}

export { OriginalWebSocket, TauriWebSocketPolyfill, SmartWebSocket };
