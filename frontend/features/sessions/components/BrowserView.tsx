"use client";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { browserStreamWsUrl } from "@/lib/control-plane-client";
import { cn } from "@/lib/utils";
import { ArrowLeft, ArrowRight, RefreshCw } from "lucide-react";
import {
	type KeyboardEvent,
	type PointerEvent,
	type WheelEvent,
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
	className?: string;
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

export function BrowserView({ sessionId, className }: BrowserViewProps) {
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
	const [canvasSize, setCanvasSize] = useState<{ width: number; height: number }>(
		{ width: 0, height: 0 },
	);
	const [urlInput, setUrlInput] = useState("");

	const wsUrl = useMemo(() => {
		if (!sessionId) return "";
		return browserStreamWsUrl(sessionId);
	}, [sessionId]);
	const isMac = useMemo(() => {
		if (typeof navigator === "undefined") return false;
		return /Mac|iPhone|iPad|iPod/.test(navigator.platform);
	}, []);
	const isConnected = connectionState === "connected";
	const primaryModifier = isMac ? 4 : 2;

	useEffect(() => {
		const container = containerRef.current;
		if (!container) return;

		const resizeObserver = new ResizeObserver((entries) => {
			for (const entry of entries) {
				const { width, height } = entry.contentRect;
				setCanvasSize({
					width: Math.max(1, Math.floor(width)),
					height: Math.max(1, Math.floor(height)),
				});
			}
		});

		resizeObserver.observe(container);
		return () => resizeObserver.disconnect();
	}, []);

	useEffect(() => {
		const canvas = canvasRef.current;
		if (!canvas) return;
		const dpr = typeof window !== "undefined" ? window.devicePixelRatio || 1 : 1;
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

	function sendMessage(payload: object) {
		const socket = socketRef.current;
		if (!socket || socket.readyState !== WebSocket.OPEN) return;
		socket.send(JSON.stringify(payload));
	}

	function sendKey(key: string, code: string | undefined, modifiers = 0) {
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
	}

	function sendChar(text: string, modifiers = 0) {
		sendMessage({
			type: "input_keyboard",
			eventType: "char",
			text,
			modifiers,
		});
	}

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
		[],
	);

	function normalizeUrl(value: string): string {
		const trimmed = value.trim();
		if (!trimmed) return "";
		if (/^[a-zA-Z][a-zA-Z0-9+.-]*:/.test(trimmed)) return trimmed;
		return `https://${trimmed}`;
	}

	const handleBack = useCallback(() => {
		if (!isConnected) return;
		if (isMac) {
			sendShortcut("[", "BracketLeft", 4);
		} else {
			sendShortcut("ArrowLeft", "ArrowLeft", 1);
		}
	}, [isConnected, isMac, sendShortcut]);

	const handleForward = useCallback(() => {
		if (!isConnected) return;
		if (isMac) {
			sendShortcut("]", "BracketRight", 4);
		} else {
			sendShortcut("ArrowRight", "ArrowRight", 1);
		}
	}, [isConnected, isMac, sendShortcut]);

	const handleReload = useCallback(() => {
		if (!isConnected) return;
		sendShortcut("r", "KeyR", primaryModifier);
	}, [isConnected, primaryModifier, sendShortcut]);

	const handleNavigate = useCallback(() => {
		if (!isConnected) return;
		const target = normalizeUrl(urlInput);
		if (!target) return;
		sendShortcut("l", "KeyL", primaryModifier);
		for (const char of target) {
			sendChar(char);
		}
		sendKey("Enter", "Enter");
	}, [isConnected, primaryModifier, sendShortcut, urlInput]);

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

	function handleWheel(e: WheelEvent<HTMLCanvasElement>) {
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

	function handleKeyDown(e: KeyboardEvent<HTMLCanvasElement>) {
		sendMessage({
			type: "input_keyboard",
			eventType: "keyDown",
			key: e.key,
			code: e.code,
			modifiers: getModifiers(e),
		});
		if (e.key.length === 1 && !e.metaKey && !e.ctrlKey) {
			sendMessage({
				type: "input_keyboard",
				eventType: "char",
				text: e.key,
				modifiers: getModifiers(e),
			});
		}
	}

	function handleKeyUp(e: KeyboardEvent<HTMLCanvasElement>) {
		sendMessage({
			type: "input_keyboard",
			eventType: "keyUp",
			key: e.key,
			code: e.code,
			modifiers: getModifiers(e),
		});
	}

	if (!sessionId) {
		return (
			<div
				className={cn(
					"h-full bg-black/70 rounded p-4 text-sm font-mono text-red-300",
					className,
				)}
			>
				Select a session to attach to the browser.
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
						placeholder="Enter URL"
						disabled={!isConnected}
						className="h-8 text-xs font-mono"
					/>
					<Button
						type="submit"
						variant="outline"
						size="sm"
						disabled={!isConnected || !urlInput.trim()}
					>
						Go
					</Button>
				</form>
			</div>
			<div
				ref={containerRef}
				className="relative flex-1 min-h-0 border border-t-0 border-border rounded-b bg-black/80 overflow-hidden"
			>
				<canvas
					ref={canvasRef}
					className="h-full w-full outline-none"
					tabIndex={0}
					onPointerDown={handlePointerDown}
					onPointerMove={handlePointerMove}
					onPointerUp={handlePointerUp}
					onPointerCancel={handlePointerUp}
					onWheel={handleWheel}
					onKeyDown={handleKeyDown}
					onKeyUp={handleKeyUp}
					style={{ touchAction: "none" }}
				/>
				{connectionState !== "connected" && (
					<div className="absolute inset-0 flex items-center justify-center text-xs text-muted-foreground bg-black/30">
						{connectionState === "connecting"
							? "Connecting to browser..."
							: connectionState === "error"
								? statusMessage || "Browser stream unavailable"
								: "Browser stream idle"}
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
