"use client";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
	Tooltip,
	TooltipContent,
	TooltipTrigger,
} from "@/components/ui/tooltip";
import {
	browserAction,
	browserStreamWsUrl,
	startBrowser,
} from "@/lib/control-plane-client";
import { cn } from "@/lib/utils";
import { ArrowLeft, ArrowRight, Globe, Loader2, Maximize2, MessageSquare, Minimize2, RefreshCw } from "lucide-react";
import { useTheme } from "next-themes";
import {
	type KeyboardEvent,
	type PointerEvent,
	useCallback,
	useEffect,
	useMemo,
	useRef,
	useState,
} from "react";

type FrameMetadata = {
	offsetTop: number;
	pageScaleFactor: number;
	deviceWidth: number;
	deviceHeight: number;
	scrollOffsetX: number;
	scrollOffsetY: number;
	timestamp?: number;
};

type StreamMessage =
	| { type: "frame"; data: string; metadata: FrameMetadata }
	| {
			type: "status";
			connected: boolean;
			screencasting: boolean;
			viewportWidth?: number;
			viewportHeight?: number;
	  }
	| { type: "error"; message: string };

type ConnectionState = "idle" | "connecting" | "connected" | "error";

type RenderTransform = {
	scale: number;
	offsetX: number;
	offsetY: number;
	deviceWidth: number;
	deviceHeight: number;
};

interface BrowserViewProps {
	sessionId?: string | null;
	workspacePath?: string | null;
	className?: string;
	/** Callback to inject text into the chat input */
	onSendToChat?: (text: string) => void;
	/** Callback to expand browser into the main panel */
	onExpand?: () => void;
	/** Callback to collapse browser back to sidebar (shown when expanded) */
	onCollapse?: () => void;
}

function getModifiers(event: {
	altKey?: boolean;
	ctrlKey?: boolean;
	metaKey?: boolean;
	shiftKey?: boolean;
}): number {
	return (
		(event.altKey ? 1 : 0) |
		(event.ctrlKey ? 2 : 0) |
		(event.metaKey ? 4 : 0) |
		(event.shiftKey ? 8 : 0)
	);
}

function mapMouseButton(button: number): "left" | "right" | "middle" | "none" {
	if (button === 0) return "left";
	if (button === 1) return "middle";
	if (button === 2) return "right";
	return "none";
}

export function BrowserView({ sessionId, workspacePath, className, onSendToChat, onExpand, onCollapse }: BrowserViewProps) {
	const canvasRef = useRef<HTMLCanvasElement | null>(null);
	const containerRef = useRef<HTMLDivElement | null>(null);
	const socketRef = useRef<WebSocket | null>(null);
	const lastFrameRef = useRef<{
		image: HTMLImageElement;
		metadata: FrameMetadata;
	} | null>(null);
	const transformRef = useRef<RenderTransform>({
		scale: 1,
		offsetX: 0,
		offsetY: 0,
		deviceWidth: 1,
		deviceHeight: 1,
	});
	const [connectionState, setConnectionState] =
		useState<ConnectionState>("idle");
	const [statusMessage, setStatusMessage] = useState<string>("");
	const [canvasSize, setCanvasSize] = useState<{
		width: number;
		height: number;
	}>({ width: 0, height: 0 });
	const [urlInput, setUrlInput] = useState("");
	const [launching, setLaunching] = useState(false);
	const [launchError, setLaunchError] = useState<string>("");
	// The octo session ID the browser daemon is bound to (returned by startBrowser)
	const [browserSessionId, setBrowserSessionId] = useState<string | null>(null);

	const wsUrl = useMemo(() => {
		if (!browserSessionId) return "";
		return browserStreamWsUrl(browserSessionId);
	}, [browserSessionId]);
	const isMac = useMemo(() => {
		if (typeof navigator === "undefined") return false;
		return /Mac|iPhone|iPad|iPod/.test(navigator.platform);
	}, []);
	const isConnected = connectionState === "connected";
	const primaryModifier = isMac ? 4 : 2;
	const { resolvedTheme } = useTheme();

	// Sync host color scheme to the browser daemon
	useEffect(() => {
		if (!isConnected || !sessionId || !resolvedTheme) return;
		const scheme = resolvedTheme === "dark" ? "dark" : "light";
		browserAction(sessionId, `color_scheme:${scheme}`).catch(() => {
			// Best-effort: ignore errors (daemon may not be ready yet)
		});
	}, [isConnected, sessionId, resolvedTheme]);

	const handleSendToChat = useCallback(() => {
		if (!browserSessionId || !onSendToChat) return;
		const cmd = [
			"The user has started a browser session you can control.",
			`Use \`octo-browser --session ${browserSessionId}\` to interact with it.`,
			"",
			"Quick reference:",
			"  octo-browser --session " + browserSessionId + " snapshot -i    # list interactive elements",
			"  octo-browser --session " + browserSessionId + " click @e1      # click element by ref",
			"  octo-browser --session " + browserSessionId + " fill @e2 \"text\" # fill input",
			"  octo-browser --session " + browserSessionId + " press Enter    # press key",
			"  octo-browser --session " + browserSessionId + " screenshot /tmp/shot.png",
			"  octo-browser --session " + browserSessionId + " open <url>     # navigate",
			"  octo-browser --session " + browserSessionId + " eval \"JS\"      # run JS in page",
		].join("\n");
		onSendToChat(cmd);
	}, [browserSessionId, onSendToChat]);

	// Reset state when workspace changes
	// biome-ignore lint/correctness/useExhaustiveDependencies: intentional reset on workspace change
	useEffect(() => {
		setBrowserSessionId(null);
		setLaunching(false);
		setLaunchError("");
		setUrlInput("");
		lastFrameRef.current = null;
	}, [workspacePath]);

	useEffect(() => {
		const container = containerRef.current;
		if (!container) return;

		const resizeObserver = new ResizeObserver((entries) => {
			for (const entry of entries) {
				const { width, height } = entry.contentRect;
				// Skip trivial sizes (container not yet laid out or hidden)
				if (width < 10 || height < 10) return;
				setCanvasSize({
					width: Math.floor(width),
					height: Math.floor(height),
				});
			}
		});

		resizeObserver.observe(container);
		return () => resizeObserver.disconnect();
	}, []);

	useEffect(() => {
		const canvas = canvasRef.current;
		if (!canvas) return;
		const dpr =
			typeof window !== "undefined" ? window.devicePixelRatio || 1 : 1;
		const targetWidth = Math.max(1, Math.floor(canvasSize.width * dpr));
		const targetHeight = Math.max(1, Math.floor(canvasSize.height * dpr));
		if (canvas.width !== targetWidth || canvas.height !== targetHeight) {
			canvas.width = targetWidth;
			canvas.height = targetHeight;
		}
		const ctx = canvas.getContext("2d");
		if (ctx) {
			ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
		}
		if (lastFrameRef.current) {
			drawFrame(lastFrameRef.current.image, lastFrameRef.current.metadata);
		}
	}, [canvasSize]);

	useEffect(() => {
		if (!wsUrl) {
			setConnectionState("idle");
			setStatusMessage("");
			if (socketRef.current) {
				socketRef.current.close();
				socketRef.current = null;
			}
			return;
		}

		setConnectionState("connecting");
		setStatusMessage("");
		const socket = new WebSocket(wsUrl);
		socketRef.current = socket;

		socket.onopen = () => {
			setConnectionState("connected");
			setStatusMessage("");
		};
		socket.onclose = () => {
			setConnectionState("idle");
		};
		socket.onerror = () => {
			setConnectionState("error");
			setStatusMessage("Stream error");
		};
		socket.onmessage = (event) => {
			try {
				const message = JSON.parse(event.data) as StreamMessage;
				if (message.type === "frame") {
					const img = new Image();
					img.onload = () => {
						lastFrameRef.current = { image: img, metadata: message.metadata };
						drawFrame(img, message.metadata);
					};
					img.src = `data:image/jpeg;base64,${message.data}`;
				} else if (message.type === "status") {
					if (!message.connected) {
						setStatusMessage("Browser not connected");
					} else if (!message.screencasting) {
						setStatusMessage("Waiting for screencast");
					} else {
						setStatusMessage("");
					}
				} else if (message.type === "error") {
					setStatusMessage(message.message);
					setConnectionState("error");
				}
			} catch (err) {
				console.warn("Failed to parse browser stream message:", err);
			}
		};

		return () => {
			socket.close();
			if (socketRef.current === socket) {
				socketRef.current = null;
			}
		};
	}, [wsUrl]);

	function drawFrame(image: HTMLImageElement, metadata: FrameMetadata) {
		const canvas = canvasRef.current;
		if (!canvas) return;
		const ctx = canvas.getContext("2d");
		if (!ctx) return;

		const viewWidth = canvasSize.width || 1;
		const viewHeight = canvasSize.height || 1;
		const deviceWidth = metadata.deviceWidth || image.width || 1;
		const deviceHeight = metadata.deviceHeight || image.height || 1;

		const scale = Math.min(viewWidth / deviceWidth, viewHeight / deviceHeight);
		const renderWidth = deviceWidth * scale;
		const renderHeight = deviceHeight * scale;
		const offsetX = (viewWidth - renderWidth) / 2;
		const offsetY = (viewHeight - renderHeight) / 2;

		ctx.clearRect(0, 0, viewWidth, viewHeight);
		ctx.drawImage(image, offsetX, offsetY, renderWidth, renderHeight);

		transformRef.current = {
			scale,
			offsetX,
			offsetY,
			deviceWidth,
			deviceHeight,
		};
	}

	const sendMessage = useCallback((payload: object) => {
		const socket = socketRef.current;
		if (!socket || socket.readyState !== WebSocket.OPEN) return;
		socket.send(JSON.stringify(payload));
	}, []);

	const sendKey = useCallback(
		(key: string, code: string | undefined, modifiers = 0) => {
			sendMessage({
				type: "input_keyboard",
				eventType: "keyDown",
				key,
				code,
				modifiers,
			});
			sendMessage({
				type: "input_keyboard",
				eventType: "keyUp",
				key,
				code,
				modifiers,
			});
		},
		[sendMessage],
	);

	const sendChar = useCallback(
		(text: string, modifiers = 0) => {
			sendMessage({
				type: "input_keyboard",
				eventType: "char",
				text,
				modifiers,
			});
		},
		[sendMessage],
	);

	const sendShortcut = useCallback(
		(key: string, code: string | undefined, modifiers: number) => {
			sendMessage({
				type: "input_keyboard",
				eventType: "keyDown",
				key,
				code,
				modifiers,
			});
			sendMessage({
				type: "input_keyboard",
				eventType: "keyUp",
				key,
				code,
				modifiers,
			});
		},
		[sendMessage],
	);

	const normalizeUrl = useCallback((value: string): string => {
		const trimmed = value.trim();
		if (!trimmed) return "";
		if (/^[a-zA-Z][a-zA-Z0-9+.-]*:/.test(trimmed)) return trimmed;
		return `https://${trimmed}`;
	}, []);

	const handleBack = useCallback(() => {
		if (!isConnected || !sessionId) return;
		browserAction(sessionId, "back").catch((err: unknown) => {
			console.warn("Failed to navigate back:", err);
		});
	}, [isConnected, sessionId]);

	const handleForward = useCallback(() => {
		if (!isConnected || !sessionId) return;
		browserAction(sessionId, "forward").catch((err: unknown) => {
			console.warn("Failed to navigate forward:", err);
		});
	}, [isConnected, sessionId]);

	const handleReload = useCallback(() => {
		if (!isConnected || !sessionId) return;
		browserAction(sessionId, "reload").catch((err: unknown) => {
			console.warn("Failed to reload:", err);
		});
	}, [isConnected, sessionId]);

	const handleNavigate = useCallback(() => {
		const target = normalizeUrl(urlInput);
		if (!target) return;
		if (!workspacePath || !sessionId) return;

		setLaunching(true);
		setLaunchError("");
		startBrowser(
			workspacePath,
			sessionId,
			target,
			canvasSize.width > 10 ? canvasSize.width : undefined,
			canvasSize.height > 10 ? canvasSize.height : undefined,
		)
			.then((result) => {
				setBrowserSessionId(result.session_id);
			})
			.catch((err: unknown) => {
				const msg =
					err instanceof Error ? err.message : "Failed to start browser";
				setLaunchError(msg);
			})
			.finally(() => {
				setLaunching(false);
			});
	}, [normalizeUrl, workspacePath, sessionId, urlInput, canvasSize]);

	function mapClientToDevice(
		clientX: number,
		clientY: number,
	): { x: number; y: number } {
		const canvas = canvasRef.current;
		if (!canvas) return { x: clientX, y: clientY };
		const rect = canvas.getBoundingClientRect();
		const x = clientX - rect.left;
		const y = clientY - rect.top;
		const { scale, offsetX, offsetY, deviceWidth, deviceHeight } =
			transformRef.current;
		const mappedX = (x - offsetX) / scale;
		const mappedY = (y - offsetY) / scale;
		return {
			x: Math.min(deviceWidth, Math.max(0, mappedX)),
			y: Math.min(deviceHeight, Math.max(0, mappedY)),
		};
	}

	function handlePointerDown(e: PointerEvent<HTMLCanvasElement>) {
		e.preventDefault();
		canvasRef.current?.focus();
		const { x, y } = mapClientToDevice(e.clientX, e.clientY);
		if (e.pointerType === "touch") {
			sendMessage({
				type: "input_touch",
				eventType: "touchStart",
				touchPoints: [{ x, y, id: e.pointerId }],
				modifiers: getModifiers(e),
			});
			return;
		}
		sendMessage({
			type: "input_mouse",
			eventType: "mousePressed",
			x,
			y,
			button: mapMouseButton(e.button),
			clickCount: 1,
			modifiers: getModifiers(e),
		});
	}

	function handlePointerMove(e: PointerEvent<HTMLCanvasElement>) {
		const { x, y } = mapClientToDevice(e.clientX, e.clientY);
		if (e.pointerType === "touch") {
			sendMessage({
				type: "input_touch",
				eventType: "touchMove",
				touchPoints: [{ x, y, id: e.pointerId }],
				modifiers: getModifiers(e),
			});
			return;
		}
		sendMessage({
			type: "input_mouse",
			eventType: "mouseMoved",
			x,
			y,
			modifiers: getModifiers(e),
		});
	}

	function handlePointerUp(e: PointerEvent<HTMLCanvasElement>) {
		const { x, y } = mapClientToDevice(e.clientX, e.clientY);
		if (e.pointerType === "touch") {
			sendMessage({
				type: "input_touch",
				eventType: "touchEnd",
				touchPoints: [{ x, y, id: e.pointerId }],
				modifiers: getModifiers(e),
			});
			return;
		}
		sendMessage({
			type: "input_mouse",
			eventType: "mouseReleased",
			x,
			y,
			button: mapMouseButton(e.button),
			clickCount: 1,
			modifiers: getModifiers(e),
		});
	}

	// Attach native listeners for wheel and contentEditable suppression.
	// - wheel: React registers as passive, preventing preventDefault()
	// - beforeinput: block contentEditable from inserting text into the canvas DOM
	// - paste: forward clipboard text to the remote browser instead
	useEffect(() => {
		const canvas = canvasRef.current;
		if (!canvas) return;
		function onWheel(e: globalThis.WheelEvent) {
			e.preventDefault();
			const { x, y } = mapClientToDevice(e.clientX, e.clientY);
			sendMessage({
				type: "input_mouse",
				eventType: "mouseWheel",
				x,
				y,
				deltaX: e.deltaX,
				deltaY: e.deltaY,
				modifiers: getModifiers(e),
			});
		}
		function onBeforeInput(e: Event) {
			e.preventDefault();
		}
		function onPaste(e: ClipboardEvent) {
			e.preventDefault();
			const text = e.clipboardData?.getData("text/plain");
			if (text) {
				for (const char of text) {
					sendMessage({
						type: "input_keyboard",
						eventType: "char",
						text: char,
						modifiers: 0,
					});
				}
			}
		}
		canvas.addEventListener("wheel", onWheel, { passive: false });
		canvas.addEventListener("beforeinput", onBeforeInput);
		canvas.addEventListener("paste", onPaste);
		return () => {
			canvas.removeEventListener("wheel", onWheel);
			canvas.removeEventListener("beforeinput", onBeforeInput);
			canvas.removeEventListener("paste", onPaste);
		};
	}, [sendMessage]);

	function handleKeyDown(e: KeyboardEvent<HTMLCanvasElement>) {
		e.preventDefault();
		sendMessage({
			type: "input_keyboard",
			eventType: "keyDown",
			key: e.key,
			code: e.code,
			keyCode: e.keyCode,
			modifiers: getModifiers(e),
		});
		if (e.key.length === 1 && !e.metaKey && !e.ctrlKey) {
			sendMessage({
				type: "input_keyboard",
				eventType: "char",
				text: e.key,
				modifiers: getModifiers(e),
			});
		} else if (e.key === "Backspace") {
			sendMessage({
				type: "input_keyboard",
				eventType: "char",
				text: "\b",
				modifiers: getModifiers(e),
			});
		} else if (e.key === "Delete") {
			sendMessage({
				type: "input_keyboard",
				eventType: "char",
				text: "\u007f",
				modifiers: getModifiers(e),
			});
		}
	}

	function handleKeyUp(e: KeyboardEvent<HTMLCanvasElement>) {
		e.preventDefault();
		sendMessage({
			type: "input_keyboard",
			eventType: "keyUp",
			key: e.key,
			code: e.code,
			keyCode: e.keyCode,
			modifiers: getModifiers(e),
		});
	}

	if (!workspacePath) {
		return (
			<div
				className={cn(
					"h-full bg-black/70 rounded p-4 text-sm font-mono text-muted-foreground flex items-center justify-center",
					className,
				)}
			>
				No workspace selected
			</div>
		);
	}

	return (
		<div className={cn("flex flex-col h-full min-h-0", className)}>
			<div className="flex items-center gap-2 px-2 py-1 border border-border rounded-t bg-muted/30">
				<Button
					type="button"
					variant="ghost"
					size="icon-sm"
					onClick={handleBack}
					disabled={!isConnected}
					title="Back"
				>
					<ArrowLeft className="size-4" />
				</Button>
				<Button
					type="button"
					variant="ghost"
					size="icon-sm"
					onClick={handleForward}
					disabled={!isConnected}
					title="Forward"
				>
					<ArrowRight className="size-4" />
				</Button>
				<Button
					type="button"
					variant="ghost"
					size="icon-sm"
					onClick={handleReload}
					disabled={!isConnected}
					title="Reload"
				>
					<RefreshCw className="size-4" />
				</Button>
				{onSendToChat && (
					<Tooltip>
						<TooltipTrigger asChild>
							<Button
								type="button"
								variant="ghost"
								size="icon-sm"
								onClick={handleSendToChat}
								disabled={!isConnected}
								title="Send browser commands to chat"
							>
								<MessageSquare className="size-4" />
							</Button>
						</TooltipTrigger>
						<TooltipContent side="bottom">
							Send browser control instructions to chat
						</TooltipContent>
					</Tooltip>
				)}
				<form
					className="flex-1 flex items-center gap-2"
					onSubmit={(event) => {
						event.preventDefault();
						handleNavigate();
					}}
				>
					<Input
						value={urlInput}
						onChange={(event) => setUrlInput(event.target.value)}
						placeholder={isConnected ? "Enter URL" : "Enter URL to start browser"}
						disabled={launching}
						className="h-8 text-xs font-mono"
					/>
					<Button
						type="submit"
						variant="outline"
						size="sm"
						disabled={launching || !urlInput.trim()}
					>
						{launching ? (
							<Loader2 className="size-4 animate-spin" />
						) : isConnected ? (
							"Go"
						) : (
							"Open"
						)}
					</Button>
				</form>
				{(onExpand || onCollapse) && (
					<Button
						type="button"
						variant="ghost"
						size="icon-sm"
						onClick={onCollapse ?? onExpand}
						title={onCollapse ? "Collapse browser" : "Expand browser"}
					>
						{onCollapse ? (
							<Minimize2 className="size-4" />
						) : (
							<Maximize2 className="size-4" />
						)}
					</Button>
				)}
			</div>
			<div
				ref={containerRef}
				className="relative flex-1 min-h-0 border border-t-0 border-border rounded-b bg-black/80 overflow-hidden"
			>
				{/* contentEditable suppresses Vimium shortcuts so all keys reach the remote browser.
				    caret-color:transparent hides the blinking text cursor that contentEditable adds. */}
				<canvas
					ref={canvasRef}
					className="h-full w-full outline-none"
					tabIndex={0}
					contentEditable
					suppressContentEditableWarning
					onPointerDown={handlePointerDown}
					onPointerMove={handlePointerMove}
					onPointerUp={handlePointerUp}
					onPointerCancel={handlePointerUp}
					onKeyDown={handleKeyDown}
					onKeyUp={handleKeyUp}
					style={{ touchAction: "none", caretColor: "transparent", userSelect: "none" }}
				/>
				{connectionState !== "connected" && (
					<div className="absolute inset-0 flex flex-col items-center justify-center gap-3 bg-black/30">
						{launching ? (
							<>
								<Loader2 className="size-6 text-muted-foreground animate-spin" />
								<span className="text-xs text-muted-foreground">
									Starting browser...
								</span>
							</>
						) : connectionState === "connecting" ? (
							<>
								<Loader2 className="size-6 text-muted-foreground animate-spin" />
								<span className="text-xs text-muted-foreground">
									Connecting to browser...
								</span>
							</>
						) : connectionState === "error" ? (
							<span className="text-xs text-destructive">
								{statusMessage || "Browser stream unavailable"}
							</span>
						) : (
							<>
								<Globe className="size-8 text-muted-foreground/50" />
								<span className="text-xs text-muted-foreground">
									Enter a URL above to start the browser
								</span>
							</>
						)}
						{launchError && (
							<span className="text-xs text-destructive max-w-[80%] text-center">
								{launchError}
							</span>
						)}
					</div>
				)}
				{statusMessage && connectionState === "connected" && (
					<div className="absolute top-2 left-2 px-2 py-1 text-[11px] text-muted-foreground bg-background/80 border border-border rounded">
						{statusMessage}
					</div>
				)}
			</div>
		</div>
	);
}
