import { RuntimeOqtoAppFrame } from "@/features/sessions/components/RuntimeOqtoAppFrame";
import { APP_PERMISSION_CHANGED_EVENT } from "@/lib/api/apps";
import { act, cleanup, render, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const state = vi.hoisted(() => ({
	fetch: vi.fn(),
	bound: true,
	permission: undefined as Promise<Response> | undefined,
}));
vi.mock("@/lib/api/client", () => ({
	authFetch: (...args: unknown[]) => state.fetch(...args),
	controlPlaneApiUrl: (path: string) => path,
}));
vi.mock("@/lib/ws-manager", () => ({
	getWsManager: () => ({ subscribeAll: () => () => {} }),
}));

const permission = () =>
	new Response(
		JSON.stringify({
			state: "allowed",
			request: {
				capabilities: [
					{
						capability: "connections",
						slots: [
							{
								slot: "api",
								expected_origin: "https://api.example.com",
								profile: "https_get_v1",
							},
						],
					},
				],
			},
		}),
		{ status: 200 },
	);

beforeEach(() => {
	state.bound = true;
	state.permission = undefined;
	state.fetch.mockReset();
	state.fetch.mockImplementation(async (url: string) => {
		if (url.includes("/permissions?")) return state.permission ?? permission();
		if (url.endsWith("/connections/invoke"))
			return state.bound
				? new Response(JSON.stringify({ items: [1] }), { status: 200 })
				: new Response(
						JSON.stringify({ error: "DO_NOT_EXPOSE_UPSTREAM_DIAGNOSTICS" }),
						{ status: 403 },
					);
		throw new Error("Unexpected route");
	});
});
afterEach(() => cleanup());

function mount(versions = ["oqto-app/v3", "oqto-app/v2"]) {
	const view = render(
		<RuntimeOqtoAppFrame
			instanceId="trusted-instance"
			installationId="installation"
			definitionId="definition"
			workspacePath="/trusted/workdir"
			html="<!doctype html><p>synthetic</p>"
			title="Fixture"
		/>,
	);
	const frame = view.container.querySelector("iframe");
	if (!frame?.contentWindow) throw new Error("Missing iframe");
	const post = vi.spyOn(frame.contentWindow, "postMessage");
	act(() =>
		window.dispatchEvent(
			new MessageEvent("message", {
				source: frame.contentWindow,
				origin: "null",
				data: {
					protocol: "oqto-app/v0",
					kind: "oqto.app.ready",
					nonce: "fixture-nonce",
					supportedVersions: versions,
				},
			}),
		),
	);
	return { view, post };
}
async function connected(versions?: string[]) {
	const mounted = mount(versions);
	await waitFor(() => expect(mounted.post).toHaveBeenCalledOnce());
	const call = mounted.post.mock.calls[0] as unknown as [
		unknown,
		string,
		MessagePort[],
	];
	const message = call?.[0] as {
		protocol: string;
		context: { capabilities: string[] };
	};
	const port = (call?.[2] as MessagePort[])[0];
	if (!port) throw new Error("No capability port");
	port.start();
	return { ...mounted, message, port };
}
function request(port: MessagePort, params: unknown): Promise<unknown> {
	return new Promise((resolve) => {
		port.onmessage = (event) => {
			if (event.data.kind === "result") resolve(event.data);
		};
		port.postMessage({
			protocol: "oqto-app/v3",
			kind: "request",
			id: 1,
			method: "connections.invoke",
			params,
		});
	});
}

describe("actual App mount -> SDK Host Bridge -> authenticated Connection facade", () => {
	it("negotiates v3 and derives instance/workdir from the parent, with slot/path only", async () => {
		const { port, message } = await connected();
		try {
			expect(message.protocol).toBe("oqto-app/v3");
			expect(message.context.capabilities).toContain("connections");
			expect(
				await request(port, { slot: "api", path: "/v1/items" }),
			).toMatchObject({ ok: true, value: { items: [1] } });
			const calls = state.fetch.mock.calls.filter(([url]) =>
				String(url).endsWith("/connections/invoke"),
			);
			expect(calls).toHaveLength(1);
			expect(calls[0]?.[0]).toBe(
				"/api/apps/instances/trusted-instance/connections/invoke",
			);
			expect(JSON.parse(calls[0]?.[1].body)).toEqual({
				workspace_path: "/trusted/workdir",
				slot: "api",
				path: "/v1/items",
			});
			expect(calls[0]?.[1].headers).toEqual({
				"Content-Type": "application/json",
			});
		} finally {
			port.close();
		}
	});

	it("rejects forged authority before any HTTP dispatch", async () => {
		const { port } = await connected();
		try {
			expect(
				await request(port, {
					slot: "api",
					path: "/v1/items",
					workspace_path: "/other",
					instance_id: "other",
					credential_id: "forged",
				}),
			).toMatchObject({ ok: false, error: { code: "invalid" } });
			expect(
				state.fetch.mock.calls.filter(([url]) =>
					String(url).endsWith("/connections/invoke"),
				),
			).toHaveLength(0);
		} finally {
			port.close();
		}
	});

	it("rechecks on each call and never forwards raw backend failure bodies", async () => {
		const { port } = await connected();
		try {
			expect(
				await request(port, { slot: "api", path: "/v1/items" }),
			).toMatchObject({ ok: true });
			state.bound = false;
			const result = await request(port, { slot: "api", path: "/v1/items" });
			expect(result).toMatchObject({ ok: false, error: { code: "denied" } });
			expect(JSON.stringify(result)).not.toContain("DO_NOT_EXPOSE");
		} finally {
			port.close();
		}
	});

	it("does not advertise Connections to a v2 App", async () => {
		const { port, message } = await connected(["oqto-app/v2", "oqto-app/v1"]);
		try {
			expect(message.protocol).toBe("oqto-app/v2");
			expect(message.context.capabilities).not.toContain("connections");
		} finally {
			port.close();
		}
	});

	it.each(["unmount", "revoke"])(
		"never creates a late bridge after %s while permission lookup is pending",
		async (reason) => {
			let release: ((value: Response) => void) | undefined;
			state.permission = new Promise<Response>((resolve) => {
				release = resolve;
			});
			const { view, post } = mount();
			await waitFor(() => expect(state.fetch).toHaveBeenCalled());
			if (reason === "unmount") view.unmount();
			else
				act(() =>
					window.dispatchEvent(
						new CustomEvent(APP_PERMISSION_CHANGED_EVENT, {
							detail: { instanceId: "trusted-instance", state: "revoked" },
						}),
					),
				);
			await act(async () => {
				release?.(permission());
				await new Promise((resolve) => setTimeout(resolve, 10));
			});
			expect(post).not.toHaveBeenCalled();
		},
	);
});
