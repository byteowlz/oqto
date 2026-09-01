// App presentations render agent-authored HTML. The entire isolation model
// rests on the iframe sandbox: srcdoc + allow-scripts WITHOUT
// allow-same-origin yields an opaque origin that cannot reach Oqto cookies,
// storage, DOM, or same-origin APIs. Per MDN, combining allow-scripts with
// allow-same-origin on same-origin-delivered content nullifies the sandbox,
// so this suite must fail loudly if anyone adds it.

import { type AppTab, AppView } from "@/features/sessions/components/AppView";
import { render } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

vi.mock("@/lib/api/apps", () => ({
	fetchAppPresentation: vi.fn(),
	listAppCandidates: vi.fn(async () => ({
		work_directory_id: "wdir",
		candidates: [],
	})),
	listAppInstances: vi.fn(async () => ({
		work_directory_id: "wdir",
		instances: [],
	})),
	publishApp: vi.fn(),
}));

const noop = () => {};

function renderTabs(tabs: AppTab[], activeTabId: string | null) {
	return render(
		<AppView
			workspacePath="/workspace/demo"
			tabs={tabs}
			activeTabId={activeTabId}
			onSetActiveTab={noop}
			onCloseTab={noop}
			onUpdateTab={noop}
			onOpenOqtoApp={noop}
		/>,
	);
}

const oqtoAppTab: AppTab = {
	kind: "oqto-app",
	id: "oqto-app:instance-1",
	instanceId: "instance-1",
	installationId: "installation-1",
	definitionId: "appdef_x",
	title: "Hello Oqto",
	html: "<!doctype html><html><body><script>document.title='x'</script></body></html>",
	pinned: false,
};

describe("runtime App presentation sandbox", () => {
	it("mounts srcdoc with allow-scripts and never allow-same-origin", () => {
		const view = renderTabs([oqtoAppTab], oqtoAppTab.id);
		const iframe = view.container.querySelector("iframe");
		expect(iframe).not.toBeNull();
		const sandbox = iframe?.getAttribute("sandbox") ?? "";
		expect(sandbox.split(" ")).toContain("allow-scripts");
		expect(sandbox).not.toContain("allow-same-origin");
		expect(sandbox).not.toContain("allow-top-navigation");
		expect(sandbox).not.toContain("allow-downloads");
		expect(iframe?.getAttribute("srcdoc")).toBe(oqtoAppTab.html);
		expect(iframe?.getAttribute("src")).toBeNull();
		expect(iframe?.getAttribute("referrerpolicy")).toBe("no-referrer");
		expect(iframe?.getAttribute("allow")).toBe("");
	});

	it("transfers one private capability port to the exact opaque frame", () => {
		const view = renderTabs([oqtoAppTab], oqtoAppTab.id);
		const iframe = view.container.querySelector("iframe");
		if (!iframe?.contentWindow) throw new Error("runtime iframe did not mount");
		const postMessage = vi.spyOn(iframe.contentWindow, "postMessage");
		const ready = new MessageEvent("message", {
			data: {
				protocol: "oqto-app/v0",
				kind: "oqto.app.ready",
				nonce: "one-time-nonce",
				supportedVersions: ["oqto-app/v1", "oqto-app/v0"],
			},
			origin: "null",
			source: iframe.contentWindow,
		});
		window.dispatchEvent(ready);

		expect(postMessage).toHaveBeenCalledOnce();
		const [connect, target, transfer] = postMessage.mock.calls[0] ?? [];
		expect(connect).toMatchObject({
			protocol: "oqto-app/v1",
			kind: "oqto.app.connect",
			nonce: "one-time-nonce",
			context: {
				instanceId: "instance-1",
				installationId: "installation-1",
				definitionId: "appdef_x",
				capabilities: ["theme", "presentation"],
			},
		});
		expect(target).toBe("*");
		expect(transfer).toHaveLength(1);
		expect(transfer?.[0]).toBeInstanceOf(MessagePort);
	});

	it("legacy html tabs also stay sandboxed without allow-same-origin", () => {
		const legacyTab: AppTab = {
			kind: "legacy-html",
			id: "legacy-1",
			filePath: "demo.html",
			title: "demo",
			content: "<html><body>legacy</body></html>",
			pinned: false,
		};
		const view = renderTabs([legacyTab], legacyTab.id);
		const iframe = view.container.querySelector("iframe");
		expect(iframe).not.toBeNull();
		const sandbox = iframe?.getAttribute("sandbox") ?? "";
		expect(sandbox.split(" ")).toContain("allow-scripts");
		expect(sandbox).not.toContain("allow-same-origin");
	});

	it("never renders App html outside a sandboxed iframe", () => {
		const view = renderTabs([oqtoAppTab], oqtoAppTab.id);
		// The document body must not contain the raw App markup as parsed DOM.
		expect(view.container.querySelector("script")).toBeNull();
	});
});
