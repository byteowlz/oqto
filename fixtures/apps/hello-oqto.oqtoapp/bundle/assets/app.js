const status = document.getElementById("status");
const count = document.getElementById("count");
const increment = document.getElementById("increment");
const reset = document.getElementById("reset");

// `randomUUID()` is secure-context-only and is not guaranteed inside an
// opaque-origin sandbox. `getRandomValues()` remains available without giving
// the App origin, storage, or network authority.
const nonceBytes = new Uint8Array(16);
crypto.getRandomValues(nonceBytes);
const nonce = Array.from(nonceBytes, (byte) => byte.toString(16).padStart(2, "0")).join("");
let port;
let protocol;
let nextId = 1;
const pending = new Map();

function showStatus(message) {
	status.textContent = message;
}

function rpc(method, params = {}) {
	if (!port) return Promise.reject(new Error("Oqto Bridge is not connected"));
	const id = nextId++;
	return new Promise((resolve, reject) => {
		pending.set(id, { resolve, reject });
		port.postMessage({ protocol, kind: "request", id, method, params });
	});
}

function updateCount(value) {
	count.textContent = String(value);
}

window.addEventListener("message", async (event) => {
	const message = event.data;
	if (
		event.source !== window.parent ||
		!message ||
		message.kind !== "oqto.app.connect" ||
		message.nonce !== nonce ||
		event.ports.length !== 1
	) return;
	protocol = message.protocol;
	port = event.ports[0];
	port.onmessage = (portEvent) => {
		const frame = portEvent.data;
		if (!frame || frame.protocol !== protocol) return;
		if (frame.kind === "suspend") {
			showStatus(`Suspended: ${frame.reason}`);
			increment.disabled = true;
			reset.disabled = true;
			for (const request of pending.values()) request.reject(new Error("App suspended"));
			pending.clear();
			return;
		}
		if (frame.kind !== "result") return;
		const request = pending.get(frame.id);
		if (!request) return;
		pending.delete(frame.id);
		if (frame.ok) request.resolve(frame.value);
		else request.reject(new Error(frame.error?.message ?? "Bridge request failed"));
	};
	port.start();
	try {
		const stored = await rpc("kv.get", { key: "counter" });
		updateCount(typeof stored === "number" ? stored : 0);
		showStatus(`Connected with ${protocol}; counter restored from private KV.`);
		increment.disabled = false;
		reset.disabled = false;
	} catch (error) {
		showStatus(error instanceof Error ? error.message : "Unable to read private KV");
	}
});

increment.addEventListener("click", async () => {
	const value = Number(count.textContent) + 1;
	increment.disabled = true;
	try {
		await rpc("kv.set", { key: "counter", value });
		updateCount(value);
		showStatus("Saved. Reload Oqto to prove persistence.");
	} catch (error) {
		showStatus(error instanceof Error ? error.message : "Save failed");
	} finally {
		increment.disabled = false;
	}
});

reset.addEventListener("click", async () => {
	try {
		await rpc("kv.delete", { key: "counter" });
		updateCount(0);
		showStatus("Counter removed from private KV.");
	} catch (error) {
		showStatus(error instanceof Error ? error.message : "Reset failed");
	}
});

window.parent.postMessage(
	{
		protocol: "oqto-app/v0",
		kind: "oqto.app.ready",
		nonce,
		supportedVersions: ["oqto-app/v2", "oqto-app/v1", "oqto-app/v0"],
	},
	"*",
);
