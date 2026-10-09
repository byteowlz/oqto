// Disposable authenticated acceptance surface. No store/response/Status substitution.
import { createInstance } from "i18next";
import { createRoot } from "react-dom/client";
import { I18nextProvider } from "react-i18next";
import en from "../messages/en.json";
import { SelectionView } from "../src/oqto-ui/chat/SelectionView";
import { createLiveSelectionHost } from "../src/oqto-ui/platform/live-selection";

async function main() {
	const params = new URLSearchParams(location.search);
	const sessionId = params.get("session");
	if (!sessionId) throw new Error("Fixture requires a platform Session route");
	// The only transport overrides are the supported production URL/auth keys.
	if (params.has("gateway"))
		localStorage.setItem("oqto:controlPlaneUrl", params.get("gateway") ?? "");
	if (params.has("token"))
		localStorage.setItem("oqto:authToken", params.get("token") ?? "");
	const { liveChatTransport: transport } = await import(
		"../src/oqto-ui/platform/live-chat-transport"
	);
	const host = createLiveSelectionHost(transport);
	const controller = host.session(sessionId);
	const events: unknown[] = [];
	const responses: unknown[] = [];
	transport.observe(null, undefined, (event) => events.push(event));
	transport.attach(sessionId, () => {}, { config: { harness: "pi" } });
	const i18n = createInstance();
	await i18n.init({
		lng: "en",
		resources: { en: { translation: en } },
		interpolation: { escapeValue: false },
	});
	const root = document.getElementById("root");
	if (!root) throw new Error("Missing fixture root");
	createRoot(root).render(
		<I18nextProvider i18n={i18n}>
			<SelectionView host={host} sessionId={sessionId} />
		</I18nextProvider>,
	);
	Object.assign(window, {
		selectionProof: {
			snapshot: () => controller.snapshot(),
			status: () => transport.status(sessionId),
			events,
			responses,
			async request(
				operation: string,
				argumentsValue: object,
				route = sessionId,
			) {
				const response = await transport.request(route, "extension_control", {
					operation,
					arguments: JSON.parse(JSON.stringify(argumentsValue)),
				});
				responses.push(response);
				return response;
			},
			reconnect() {
				transport.disconnect();
				transport.attach(sessionId, () => {}, { config: { harness: "pi" } });
			},
			outbox: () =>
				JSON.parse(localStorage.getItem("oqto:chatOutbox:v1") ?? "[]"),
		},
	});
}
void main();
