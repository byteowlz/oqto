import { CredentialSettings } from "@/components/settings/CredentialSettings";
import {
	canProvisionCredentials,
	isCredentialTransportSafe,
} from "@/lib/api/credentials";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import {
	cleanup,
	fireEvent,
	render,
	screen,
	waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const transport = vi.hoisted(() => ({
	base: "https://host.test",
	fetch: vi.fn(),
}));
vi.mock("@/lib/api/client", () => ({
	authFetch: transport.fetch,
	controlPlaneApiUrl: (path: string) => transport.base + path,
}));

function response(
	status: string,
	entries: { id: string; revision: number }[] = [],
	code: string | null = null,
) {
	return Promise.resolve(
		new Response(JSON.stringify({ status, entries, code }), { status: 200 }),
	);
}
function submit(label: string, value: string) {
	const input = screen.getByLabelText(label) as HTMLInputElement;
	fireEvent.change(input, { target: { value } });
	if (!input.form) throw new Error("Missing credential form");
	fireEvent.submit(input.form);
	expect(input.value).toBe("");
}
function payload() {
	return JSON.parse(transport.fetch.mock.calls.at(-1)?.[1].body);
}

beforeEach(() => {
	transport.fetch.mockReset();
	transport.base = "https://host.test";
});
afterEach(() => {
	cleanup();
	vi.restoreAllMocks();
	vi.unstubAllGlobals();
});

describe("host credential settings", () => {
	it("rejects an actual remote HTTP page even with secure-context override", () => {
		vi.stubGlobal("window", {
			location: { href: "http://remote.test" },
			isSecureContext: true,
		});
		expect(canProvisionCredentials()).toBe(false);
	});
	it("initializes, explicitly locks/unlocks and clears secrets without persistence", async () => {
		const storage = vi.spyOn(Storage.prototype, "setItem");
		transport.fetch.mockImplementationOnce(() => response("uninitialized"));
		const cache = new QueryClient();
		const queryWrites = vi.spyOn(cache, "setQueryData");
		render(
			<QueryClientProvider client={cache}>
				<CredentialSettings />
			</QueryClientProvider>,
		);
		await screen.findByText("Vault: uninitialized");
		transport.fetch.mockImplementationOnce(() => response("ready"));
		submit("Vault passphrase", "private-passphrase");
		expect(payload()).toEqual({
			action: "initialize",
			passphrase: "private-passphrase",
		});
		await screen.findByText("Vault: ready");
		expect(screen.queryByText("private-passphrase")).toBeNull();
		transport.fetch.mockImplementationOnce(() => response("locked"));
		fireEvent.click(screen.getByText("Lock vault"));
		expect(payload()).toEqual({ action: "lock" });
		await screen.findByText("Vault: locked");
		transport.fetch.mockImplementationOnce(() => response("ready"));
		submit("Vault passphrase", "unlock-passphrase");
		expect(payload()).toEqual({
			action: "unlock",
			passphrase: "unlock-passphrase",
		});
		await screen.findByText("Vault: ready");
		expect(storage).not.toHaveBeenCalled();
		expect(queryWrites).not.toHaveBeenCalled();
		expect(cache.getQueryCache().getAll()).toEqual([]);
		expect(cache.getMutationCache().getAll()).toEqual([]);
	});
	it("saves, updates and deletes only with explicit revision metadata", async () => {
		transport.fetch.mockImplementationOnce(() => response("ready"));
		render(<CredentialSettings />);
		await screen.findByText("Vault: ready");
		fireEvent.change(screen.getByLabelText("Credential ID"), {
			target: { value: "service" },
		});
		transport.fetch.mockImplementationOnce(() =>
			response("ready", [{ id: "service", revision: 4 }]),
		);
		submit("New secret value", "secret-one");
		expect(payload()).toEqual({
			action: "put",
			id: "service",
			value: "secret-one",
			expected_revision: null,
		});
		await screen.findByText("Update service");
		fireEvent.click(screen.getByText("Update service"));
		transport.fetch.mockImplementationOnce(() =>
			response("ready", [{ id: "service", revision: 5 }]),
		);
		submit("New secret value", "secret-two");
		expect(payload()).toEqual({
			action: "put",
			id: "service",
			value: "secret-two",
			expected_revision: 4,
		});
		await screen.findByText("service · revision 5");
		transport.fetch.mockImplementationOnce(() => response("ready"));
		fireEvent.click(screen.getByText("Delete service"));
		expect(payload()).toEqual({
			action: "delete",
			id: "service",
			expected_revision: 5,
		});
		await waitFor(() =>
			expect(screen.queryByText("Delete service")).toBeNull(),
		);
	});
	it("blocks insecure remote API despite secure context and clears rejected inputs", async () => {
		transport.base = "http://remote.test";
		transport.fetch.mockImplementationOnce(() => response("locked"));
		render(<CredentialSettings />);
		await screen.findByText("Vault: locked");
		submit("Vault passphrase", "never-send");
		expect(transport.fetch).toHaveBeenCalledTimes(1);
		expect(isCredentialTransportSafe("http://remote.test")).toBe(false);
		expect(isCredentialTransportSafe("http://localhost:3000")).toBe(true);
		expect(isCredentialTransportSafe("http://127.0.0.1:3000")).toBe(true);
		expect(isCredentialTransportSafe("http://[::1]:3000")).toBe(true);
		expect(isCredentialTransportSafe("http://localhost.evil.test")).toBe(false);
	});
	it("does not reflect server error bodies and requires refresh after failure", async () => {
		transport.fetch.mockImplementationOnce(() => response("locked"));
		render(<CredentialSettings />);
		await screen.findByText("Vault: locked");
		transport.fetch.mockResolvedValueOnce(
			new Response("private-passphrase", { status: 500 }),
		);
		submit("Vault passphrase", "private-passphrase");
		await screen.findByText("Credential status unavailable");
		expect(screen.queryByText("private-passphrase")).toBeNull();
	});
	it("clears an unsubmitted secret on unmount without sending it", async () => {
		transport.fetch.mockImplementationOnce(() => response("locked"));
		const view = render(<CredentialSettings />);
		await screen.findByText("Vault: locked");
		const input = screen.getByLabelText("Vault passphrase") as HTMLInputElement;
		fireEvent.change(input, { target: { value: "unsent-secret" } });
		view.unmount();
		expect(input.value).toBe("");
		expect(transport.fetch).toHaveBeenCalledTimes(1);
	});
	it("clears a pending secret input and aborts mutation on unmount", async () => {
		transport.fetch.mockImplementationOnce(() => response("locked"));
		const view = render(<CredentialSettings />);
		await screen.findByText("Vault: locked");
		const input = screen.getByLabelText("Vault passphrase") as HTMLInputElement;
		transport.fetch.mockImplementationOnce(() => new Promise(() => {}));
		submit("Vault passphrase", "temporary-secret");
		const signal = transport.fetch.mock.calls.at(-1)?.[1].signal as AbortSignal;
		view.unmount();
		expect(input.value).toBe("");
		expect(signal.aborted).toBe(true);
	});
	it("aborts pending lifecycle work on unmount and shows unavailable without fake provider", async () => {
		transport.fetch.mockImplementationOnce(() => response("unavailable"));
		const view = render(<CredentialSettings />);
		await screen.findByText("Vault: unavailable");
		expect(screen.queryByLabelText("Vault passphrase")).toBeNull();
		const signal = transport.fetch.mock.calls[0][1].signal as AbortSignal;
		view.unmount();
		expect(signal.aborted).toBe(true);
	});
});
