import type { JsonValue } from "../engine/projection";
import type { ChatTransport } from "../engine/transport";
import { createSelectionController } from "./selection-controller";
import type { SelectionPort } from "./selection-port";
import type { SelectionHost } from "./selection-service";

function object(value: unknown): Record<string, JsonValue | undefined> {
	if (!value || typeof value !== "object" || Array.isArray(value))
		throw new Error("Invalid control acknowledgement");
	return value as Record<string, JsonValue | undefined>;
}
const commands = [
	"presentation-bind",
	"presentation-unbind",
	"selection-get",
	"selection-save",
	"selection-submit",
	"selection-close",
	"selection-ack",
];
export function createLiveSelectionHost(
	transport: ChatTransport,
): SelectionHost {
	const sessions = new Map<
		string,
		ReturnType<typeof createSelectionController>
	>();
	return {
		session(sessionId) {
			const existing = sessions.get(sessionId);
			if (existing) return existing;
			let available = false;
			const port: SelectionPort = {
				async discover() {
					const response = await transport.request(sessionId, "get_commands");
					available =
						response.success === true &&
						Array.isArray(response.data) &&
						commands.every(
							(name) =>
								Array.isArray(response.data) &&
								response.data.some(
									(value) =>
										value &&
										typeof value === "object" &&
										!Array.isArray(value) &&
										value.name === name,
								),
						);
					return available;
				},
				async control(operation, argumentsValue) {
					if (!available) throw new Error("Selection controls are unavailable");
					const response = await transport.request(
						sessionId,
						"extension_control",
						{
							operation,
							arguments: JSON.parse(
								JSON.stringify(argumentsValue),
							) as JsonValue,
						},
					);
					if (!response.success || response.session_id !== sessionId)
						throw new Error(
							typeof response.error === "string"
								? response.error
								: "Selection control rejected",
						);
					const data = object(response.data);
					if (
						data.operation !== operation ||
						typeof data.request_id !== "string" ||
						data.request_id.length > 128 ||
						!data.request_id ||
						typeof data.native_session_id !== "string" ||
						data.native_session_id.length > 512 ||
						!data.native_session_id
					)
						throw new Error("Mismatched control acknowledgement");
					const reply = object(data.reply);
					if (
						reply.version !== 1 ||
						reply.command !== operation.replaceAll("_", "-") ||
						typeof reply.requestId !== "string" ||
						!reply.requestId ||
						reply.requestId.length > 128 ||
						reply.ok !== true
					)
						throw new Error(
							typeof reply.error === "string"
								? reply.error
								: "Selection control rejected",
						);
					if (operation === "presentation_bind") {
						const snapshot = object(reply.snapshot);
						if (
							snapshot.scopeId !== data.native_session_id ||
							!Array.isArray(snapshot.bindings)
						)
							throw new Error("Invalid presentation scope");
						const binding = snapshot.bindings
							.map(object)
							.find((row) => row.id === data.binding_id);
						if (
							!binding ||
							typeof binding.expiresAt !== "number" ||
							binding.expiresAt <= Date.now()
						)
							throw new Error("Missing or expired presentation lease");
						return { ...reply, bindingId: data.binding_id };
					}
					return reply.result ?? reply;
				},
				listen(present, connection) {
					const warnUnsaved = (event: BeforeUnloadEvent) => {
						if (controller.snapshot().dirty) {
							event.preventDefault();
							event.returnValue = "";
						}
					};
					window.addEventListener("beforeunload", warnUnsaved);
					let lastReady: boolean | null = null;
					const readiness = () => {
						const status = transport.status(sessionId);
						const ready =
							status.connection === "connected" && status.sessionReady;
						if (lastReady !== ready) {
							lastReady = ready;
							if (!ready) available = false;
							connection(ready);
						}
					};
					const stopEvents = transport.observe(null, undefined, (event) => {
						if (event.session_id === sessionId) readiness();
						if (
							event.session_id !== sessionId ||
							event.event !== "status" ||
							event.key !== "pi-selection:present/v1" ||
							typeof event.text !== "string" ||
							event.text.length > 262144
						)
							return;
						try {
							present(JSON.parse(event.text));
						} catch {
							present(null);
						}
					});
					const stopConnection = transport.observe(readiness);
					readiness();
					return () => {
						stopEvents();
						stopConnection();
						window.removeEventListener("beforeunload", warnUnsaved);
					};
				},
				resume(scopeId) {
					try {
						const pointer = object(
							JSON.parse(
								window.sessionStorage.getItem(`oqto:selection:${sessionId}`) ??
									"null",
							),
						);
						return pointer.scopeId === scopeId && typeof pointer.id === "string"
							? pointer.id
							: null;
					} catch {
						return null;
					}
				},
				remember(scopeId, id) {
					try {
						window.sessionStorage.setItem(
							`oqto:selection:${sessionId}`,
							JSON.stringify({ scopeId, id }),
						);
					} catch {
						/* Pointer is optional; records never enter browser storage. */
					}
				},
				renew(callback) {
					const timer = setTimeout(callback, 30000);
					return () => clearTimeout(timer);
				},
			};
			const controller = createSelectionController(port);
			sessions.set(sessionId, controller);
			return controller;
		},
	};
}
