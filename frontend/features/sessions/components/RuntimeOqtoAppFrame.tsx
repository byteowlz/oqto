import {
	APP_PERMISSION_CHANGED_EVENT,
	type AppPermissionChangedDetail,
	deleteAppKv,
	getAppKv,
	getAppPermissions,
	setAppKv,
} from "@/lib/api/apps";
import { getWsManager } from "@/lib/ws-manager";
import {
	type JsonValue,
	OQTO_APP_PROTOCOL,
	OQTO_APP_PROTOCOL_V1,
	OQTO_APP_PROTOCOL_V2,
	type OqtoCapability,
	type OqtoHostContext,
	type OqtoPresentationContext,
	type OqtoThemeSnapshot,
	type OqtoUnsubscribe,
} from "@byteowlz/oqto-app-sdk";
import {
	type OqtoHostBridge,
	serveOqtoAppPort,
} from "@byteowlz/oqto-app-sdk/host";
import { useCallback, useRef } from "react";

interface RuntimeOqtoAppFrameProps {
	instanceId: string;
	installationId: string;
	definitionId: string;
	html: string;
	title: string;
	workspacePath: string;
}

const THEME_TOKENS = [
	"--background",
	"--foreground",
	"--card",
	"--card-foreground",
	"--primary",
	"--primary-foreground",
	"--muted",
	"--muted-foreground",
	"--border",
] as const;

function themeSnapshot(): OqtoThemeSnapshot {
	const root = document.documentElement;
	const styles = getComputedStyle(root);
	return {
		colorScheme: root.classList.contains("dark") ? "dark" : "light",
		tokens: Object.fromEntries(
			THEME_TOKENS.map((token) => [
				token,
				styles.getPropertyValue(token).trim(),
			]),
		),
	};
}

function watchTheme(
	listener: (theme: OqtoThemeSnapshot) => void,
): Promise<OqtoUnsubscribe> {
	const observer = new MutationObserver(() => listener(themeSnapshot()));
	observer.observe(document.documentElement, {
		attributes: true,
		attributeFilter: ["class", "style", "data-theme"],
	});
	return Promise.resolve(() => observer.disconnect());
}

function presentationContext(
	frame: HTMLIFrameElement,
): OqtoPresentationContext {
	const bounds = frame.getBoundingClientRect();
	const width = Math.max(0, bounds.width);
	const height = Math.max(0, bounds.height);
	return {
		surface: "container",
		width,
		height,
		sizeClass: width < 600 ? "compact" : width < 1024 ? "regular" : "expanded",
		density: "compact",
		safeArea: { top: 0, right: 0, bottom: 0, left: 0 },
		reducedMotion: window.matchMedia("(prefers-reduced-motion: reduce)")
			.matches,
	};
}

function watchPresentation(
	frame: HTMLIFrameElement,
	listener: (context: OqtoPresentationContext) => void,
): Promise<OqtoUnsubscribe> {
	const reducedMotion = window.matchMedia("(prefers-reduced-motion: reduce)");
	const emit = () => listener(presentationContext(frame));
	const observer = new ResizeObserver(emit);
	observer.observe(frame);
	reducedMotion.addEventListener("change", emit);
	return Promise.resolve(() => {
		observer.disconnect();
		reducedMotion.removeEventListener("change", emit);
	});
}

export function RuntimeOqtoAppFrame({
	instanceId,
	installationId,
	definitionId,
	html,
	title,
	workspacePath,
}: RuntimeOqtoAppFrameProps) {
	const cleanupRef = useRef<(() => void) | null>(null);
	const setFrame = useCallback(
		(frame: HTMLIFrameElement | null) => {
			cleanupRef.current?.();
			cleanupRef.current = null;
			if (!frame) return;

			let bridge: OqtoHostBridge | undefined;
			let accepted = false;
			const suspendIfWithdrawn = (detail: AppPermissionChangedDetail) => {
				if (
					detail.instanceId === instanceId &&
					detail.state !== "allowed" &&
					detail.state !== "not_required"
				) {
					bridge?.suspend({
						reason: detail.state === "revoked" ? "revoked" : "suspended",
						message: "Oqto App permission was withdrawn",
					});
				}
			};
			const onPermissionChanged = (event: Event) => {
				suspendIfWithdrawn(
					(event as CustomEvent<AppPermissionChangedDetail>).detail,
				);
			};
			window.addEventListener(
				APP_PERMISSION_CHANGED_EVENT,
				onPermissionChanged,
			);
			const unsubscribeLifecycle = getWsManager().subscribeAll((event) => {
				if (event.channel === "system" && event.type === "app.lifecycle") {
					suspendIfWithdrawn({
						instanceId: event.instance_id,
						state: event.state as AppPermissionChangedDetail["state"],
					});
				}
			});
			let lifecycleChannel: BroadcastChannel | undefined;
			try {
				lifecycleChannel = new BroadcastChannel(APP_PERMISSION_CHANGED_EVENT);
				lifecycleChannel.onmessage = (event) =>
					suspendIfWithdrawn(event.data as AppPermissionChangedDetail);
			} catch {
				// The current document still receives lifecycle events.
			}
			const onMessage = (event: MessageEvent<unknown>) => {
				if (
					accepted ||
					event.source !== frame.contentWindow ||
					event.origin !== "null"
				)
					return;
				const message = event.data;
				if (
					typeof message !== "object" ||
					message === null ||
					!("protocol" in message) ||
					message.protocol !== OQTO_APP_PROTOCOL ||
					!("kind" in message) ||
					message.kind !== "oqto.app.ready" ||
					!("nonce" in message) ||
					typeof message.nonce !== "string" ||
					message.nonce.length === 0 ||
					message.nonce.length > 512
				) {
					return;
				}
				const offered =
					"supportedVersions" in message &&
					Array.isArray(message.supportedVersions)
						? message.supportedVersions
						: [OQTO_APP_PROTOCOL];
				const protocol = offered.includes(OQTO_APP_PROTOCOL_V2)
					? OQTO_APP_PROTOCOL_V2
					: offered.includes(OQTO_APP_PROTOCOL_V1)
						? OQTO_APP_PROTOCOL_V1
						: offered.includes(OQTO_APP_PROTOCOL)
							? OQTO_APP_PROTOCOL
							: undefined;
				if (!protocol) return;

				accepted = true;
				void (async () => {
					const permission = await getAppPermissions(workspacePath, instanceId);
					if (
						permission.state !== "allowed" &&
						permission.state !== "not_required"
					)
						return;
					const granted = permission.request.capabilities.map(
						(capability) => capability.capability,
					) as OqtoCapability[];
					const capabilities = [...granted, "presentation"] as OqtoCapability[];
					const context: OqtoHostContext = {
						protocol,
						instanceId,
						installationId,
						definitionId,
						capabilities,
						grants: {
							capabilities,
							resources: [],
							operations: [],
						},
						presentation: presentationContext(frame),
					};
					const channel = new MessageChannel();
					bridge = serveOqtoAppPort(
						{
							context,
							...(granted.includes("kv")
								? {
										kv: {
											get: (key: string) =>
												getAppKv(workspacePath, instanceId, key) as Promise<
													JsonValue | undefined
												>,
											set: (key: string, value: JsonValue) =>
												setAppKv(workspacePath, instanceId, key, value),
											delete: (key: string) =>
												deleteAppKv(workspacePath, instanceId, key),
										},
									}
								: {}),
							...(granted.includes("theme")
								? {
										theme: {
											get: async () => themeSnapshot(),
											watch: watchTheme,
										},
									}
								: {}),
							presentation: {
								get: async () => presentationContext(frame),
								watch: (listener) => watchPresentation(frame, listener),
							},
						},
						channel.port1,
					);
					frame.contentWindow?.postMessage(
						{
							protocol,
							kind: "oqto.app.connect",
							nonce: message.nonce,
							context,
						},
						"*",
						[channel.port2],
					);
				})().catch(() => {
					bridge?.close();
				});
			};

			window.addEventListener("message", onMessage);
			frame.srcdoc = html;
			cleanupRef.current = () => {
				window.removeEventListener("message", onMessage);
				window.removeEventListener(
					APP_PERMISSION_CHANGED_EVENT,
					onPermissionChanged,
				);
				lifecycleChannel?.close();
				unsubscribeLifecycle();
				bridge?.close();
			};
		},
		[definitionId, html, installationId, instanceId, workspacePath],
	);

	return (
		<iframe
			ref={setFrame}
			title={title}
			className="h-full w-full border-0 bg-background"
			sandbox="allow-scripts"
			referrerPolicy="no-referrer"
			allow=""
		/>
	);
}
