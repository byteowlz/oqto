import {
	APP_PERMISSION_CHANGED_EVENT,
	type AppPermissionChangedDetail,
	deleteAppKv,
	getAppFileResources,
	getAppKv,
	getAppPermissions,
	invokeAppOperation,
	listAppFiles,
	readAppFile,
	setAppKv,
	writeAppFile,
} from "@/lib/api/apps";
import { getWsManager } from "@/lib/ws-manager";
import {
	type JsonValue,
	OQTO_APP_PROTOCOL,
	OQTO_APP_PROTOCOL_V1,
	OQTO_APP_PROTOCOL_V2,
	type OqtoCapability,
	type OqtoFileRef,
	type OqtoFileVersion,
	type OqtoFilesCapability,
	type OqtoGrantedOperation,
	type OqtoGrantedResource,
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

function decodeBase64(value: string): Uint8Array {
	const binary = atob(value);
	return Uint8Array.from(binary, (character) => character.charCodeAt(0));
}

function createFilesCapability(
	workspacePath: string,
	instanceId: string,
	resources: readonly OqtoGrantedResource[],
): OqtoFilesCapability {
	const contents = async (ref: OqtoFileRef) => {
		const file = await readAppFile(workspacePath, instanceId, ref);
		return {
			ref,
			version: file.version as OqtoFileVersion,
			label: file.label,
			mediaType: file.media_type,
			size: file.size,
			access:
				resources.find((resource) =>
					String(ref).startsWith(String(resource.ref)),
				)?.access ?? "read",
			modifiedAt: file.modified_at,
			bytes: decodeBase64(file.bytes_base64),
		};
	};
	return {
		pick: async () => [],
		read: contents,
		stat: async (ref) => {
			const { bytes: _, ...stat } = await contents(ref);
			return stat;
		},
		write: async (ref, bytes, options) => {
			const result = await writeAppFile(
				workspacePath,
				instanceId,
				ref,
				options.expectedVersion,
				bytes,
			);
			if (!result.written) {
				return {
					ok: false as const,
					reason: "conflict" as const,
					currentVersion: result.version as OqtoFileVersion,
				};
			}
			const stat = await contents(ref);
			const { bytes: _, ...fileStat } = stat;
			return { ok: true as const, stat: fileStat };
		},
		watch: async (ref, listener) => {
			let previous: string | undefined;
			let generation = 0;
			const poll = async () => {
				try {
					const stat = await contents(ref);
					if (previous !== undefined && previous !== stat.version) {
						generation += 1;
						listener({ ref, version: stat.version, generation, gap: false });
					}
					previous = stat.version;
				} catch {
					// A lifecycle revocation or transient absence fails closed.
				}
			};
			void poll();
			const timer = window.setInterval(() => void poll(), 2000);
			return () => window.clearInterval(timer);
		},
		resources: async () => resources,
		list: async (ref, options) => {
			const entries = await listAppFiles(workspacePath, instanceId, ref);
			const limit = Math.max(1, Math.min(options?.limit ?? 100, 256));
			return {
				entries: entries.slice(0, limit).map((entry) => ({
					ref: entry.reference as OqtoFileRef,
					label: entry.label,
					mediaType: entry.media_type,
					access:
						resources.find((resource) =>
							entry.reference.startsWith(String(resource.ref)),
						)?.access ?? "read",
					version: entry.version as OqtoFileVersion,
					size: entry.size,
					modifiedAt: entry.modified_at,
				})),
			};
		},
		watchResources: async (refs, listener) => {
			const unsubscribes = await Promise.all(
				refs.map((ref) =>
					createFilesCapability(workspacePath, instanceId, resources).watch(
						ref,
						listener,
					),
				),
			);
			return () => {
				for (const unsubscribe of unsubscribes) unsubscribe();
			};
		},
	};
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
					const resourceGrants = granted.includes("files")
						? ((await getAppFileResources(workspacePath, instanceId)).map(
								(resource) => ({
									ref: resource.reference as OqtoFileRef,
									role: resource.role,
									label: resource.label,
									mediaType:
										resource.kind === "collection"
											? "inode/directory"
											: "application/octet-stream",
									access: resource.access,
									kind: resource.kind,
									watch: resource.watch,
								}),
							) as OqtoGrantedResource[])
						: [];
					const operationGrants = permission.request.capabilities
						.filter((capability) => capability.capability === "operations")
						.flatMap(
							(capability) => capability.operations,
						) as OqtoGrantedOperation[];
					const capabilities = [
						...granted.filter((capability) => capability !== "theme"),
						"theme",
						"presentation",
					] as OqtoCapability[];
					const context: OqtoHostContext = {
						protocol,
						instanceId,
						installationId,
						definitionId,
						capabilities,
						grants: {
							capabilities,
							resources: resourceGrants,
							operations: operationGrants,
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
							...(granted.includes("files")
								? {
										files: createFilesCapability(
											workspacePath,
											instanceId,
											resourceGrants,
										),
									}
								: {}),
							...(granted.includes("operations")
								? {
										operations: {
											list: async () => operationGrants,
											invoke: async (
												id: string,
												input: JsonValue = null,
												options?: { signal?: AbortSignal },
											) => {
												const result = await invokeAppOperation(
													workspacePath,
													instanceId,
													id,
													input,
													options?.signal,
												);
												return result.ok
													? {
															ok: true as const,
															output: result.output as JsonValue,
														}
													: {
															ok: false as const,
															reason: "failed" as const,
															code: result.code,
															message: result.message,
														};
											},
										},
									}
								: {}),
							theme: {
								get: async () => themeSnapshot(),
								watch: watchTheme,
							},
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
