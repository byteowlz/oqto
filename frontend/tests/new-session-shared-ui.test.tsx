import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import {
	act,
	fireEvent,
	render,
	screen,
	waitFor,
} from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { initI18n } from "../lib/i18n";
import { CompositorShell } from "../src/oqto-ui/app/CompositorShell";
import { OqtoUiShell } from "../src/oqto-ui/app/OqtoUiShell";
import { scriptedOqtoUiPlatform } from "../src/oqto-ui/dev/scripted-platform";
import type {
	OqtoUiPlatform,
	OqtoUiSnapshot,
} from "../src/oqto-ui/platform/contracts";
import { memoryLayoutStorage } from "../src/oqto-ui/platform/layout-storage";
import { NavigationRail } from "../src/oqto-ui/sessions/NavigationRail";

initI18n();
beforeEach(() => {
	const values = new Map<string, string>();
	vi.mocked(localStorage.getItem).mockImplementation(
		(key) => values.get(key) ?? null,
	);
	vi.mocked(localStorage.setItem).mockImplementation((key, value) => {
		values.set(key, value);
	});
});
function deferred<T>() {
	let resolve!: (value: T) => void;
	let reject!: (error: Error) => void;
	const promise = new Promise<T>((yes, no) => {
		resolve = yes;
		reject = no;
	});
	return { promise, resolve, reject };
}
function client() {
	return new QueryClient({
		defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
	});
}
async function rail(createSession?: OqtoUiPlatform["createSession"]) {
	const snapshot = await scriptedOqtoUiPlatform.load(null);
	const directory = snapshot.workDirectories[0];
	const navigate = vi.fn();
	const queryClient = client();
	const invalidate = vi.spyOn(queryClient, "invalidateQueries");
	const send = vi.fn();
	const platform = {
		...scriptedOqtoUiPlatform,
		chat: { ...scriptedOqtoUiPlatform.chat, send },
		id: "fake-creation",
		createSession,
	};
	render(
		<QueryClientProvider client={queryClient}>
			<NavigationRail
				platform={platform}
				workDirectories={snapshot.workDirectories}
				workDirectoryId={directory.id}
				sessionId={directory.sessions[0].id}
				schemeId="oqto-dark"
				onNavigate={navigate}
			/>
		</QueryClientProvider>,
	);
	return {
		button: screen.getByRole("button", { name: "New session" }),
		navigate,
		invalidate,
		directory,
		send,
	};
}
describe("shared New Session capability", () => {
	it("truthfully disables unsupported hosts", async () => {
		const { button } = await rail();
		expect(button).toBeDisabled();
		expect(button).toHaveAttribute(
			"title",
			"This host does not support creating sessions",
		);
	});
	it("guards rapid repeats, waits for committed IDs and invalidates the host catalog", async () => {
		const ack = deferred<{ sessionId: string; workDirectoryId: string }>();
		const create = vi.fn(() => ack.promise);
		const { button, navigate, invalidate, directory, send } =
			await rail(create);
		fireEvent.click(button);
		fireEvent.click(button);
		expect(create).toHaveBeenCalledExactlyOnceWith(
			directory.id,
			directory.sessions[0].id,
		);
		expect(navigate).not.toHaveBeenCalled();
		expect(button).toBeDisabled();
		await act(async () =>
			ack.resolve({
				sessionId: "opaque/session?B",
				workDirectoryId: "opaque:work/B",
			}),
		);
		expect(navigate).toHaveBeenCalledExactlyOnceWith({
			sessionId: "opaque/session?B",
			workDirectoryId: "opaque:work/B",
		});
		expect(invalidate).toHaveBeenCalledWith({
			queryKey: ["oqto-ui", "fake-creation"],
		});
		expect(button).toBeEnabled();
		expect(send).not.toHaveBeenCalled();
	});
	it("unknown outcomes keep the route, announce uncertainty, refresh and allow retry", async () => {
		const ack = deferred<{ sessionId: string; workDirectoryId: string }>();
		const { button, navigate, invalidate } = await rail(() => ack.promise);
		fireEvent.click(button);
		await act(async () => ack.reject(new Error("ACK lost")));
		expect(navigate).not.toHaveBeenCalled();
		expect(screen.getByRole("alert")).toHaveTextContent("not confirmed");
		expect(invalidate).toHaveBeenCalledWith({
			queryKey: ["oqto-ui", "fake-creation"],
		});
		expect(button).toBeEnabled();
	});
});

for (const Shell of [OqtoUiShell, CompositorShell]) {
	describe(`${Shell.name} requested Session fence`, () => {
		it.each(["reject", "wrong-identity"])(
			"does not show A as B during load or %s",
			async (outcome) => {
				const snapshot = await scriptedOqtoUiPlatform.load(null);
				const a = snapshot.activeSessionId;
				const next = deferred<OqtoUiSnapshot>();
				const load = vi.fn((id: string | null) =>
					id === a ? Promise.resolve(snapshot) : next.promise,
				);
				const platform = {
					...scriptedOqtoUiPlatform,
					id: `fence-${Shell.name}`,
					load,
				};
				const queryClient = client();
				const props = {
					storage: memoryLayoutStorage(),
					platform,
					workDirectoryId: null,
					mobileView: "chat",
					schemeId: null,
					workAreaTab: "chat",
					onNavigate: vi.fn(),
				};
				const view = render(
					<QueryClientProvider client={queryClient}>
						<Shell {...props} sessionId={a} />
					</QueryClientProvider>,
				);
				await screen.findByRole("main", { name: "Session conversation" });
				view.rerender(
					<QueryClientProvider client={queryClient}>
						<Shell {...props} sessionId="unknown-public-B" />
					</QueryClientProvider>,
				);
				expect(
					screen.queryByRole("main", { name: "Session conversation" }),
				).toBeNull();
				expect(view.container.querySelector("textarea")).toBeNull();
				await act(async () => {
					if (outcome === "reject") next.reject(new Error("unknown ID"));
					else next.resolve(snapshot);
				});
				await waitFor(() =>
					expect(
						screen.getByRole("button", { name: /retry/i }),
					).toBeInTheDocument(),
				);
				expect(view.container.querySelector("textarea")).toBeNull();
			},
		);
	});
}
