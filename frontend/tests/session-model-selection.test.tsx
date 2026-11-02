import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import {
	act,
	fireEvent,
	render,
	screen,
	waitFor,
} from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { initI18n } from "../lib/i18n";
import type {
	OqtoUiPlatform,
	SessionOverview,
} from "../src/oqto-ui/platform/contracts";
import { SessionStatusBar } from "../src/oqto-ui/sessions/SessionStatusBar";

initI18n();
const session: SessionOverview = {
	id: "native-session",
	name: "Test",
	preview: "",
	updated: "",
	status: "idle",
	model: "old",
};
const models = [
	{ id: "old", name: "Saved model" },
	{ id: "discovered", name: "Discovered model" },
];

function setup(selectModel?: OqtoUiPlatform["selectModel"]) {
	const client = new QueryClient({
		defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
	});
	const invalidate = vi.spyOn(client, "invalidateQueries");
	const platform = { id: "test-platform", selectModel };
	function view(current: SessionOverview) {
		return (
			<QueryClientProvider client={client}>
				<SessionStatusBar
					status={null}
					session={current}
					models={models}
					platform={platform}
					settingsOpen={false}
					onToggleSettings={() => {}}
				/>
			</QueryClientProvider>
		);
	}
	const rendered = render(view(session));
	const trigger = () => screen.getByRole("button", { name: "Model" });
	return {
		invalidate,
		trigger,
		rerender: (current: SessionOverview) => rendered.rerender(view(current)),
	};
}

describe("canonical status-bar model selection", () => {
	it("disables selection when the adapter has no capability", () => {
		const { trigger, invalidate } = setup();
		expect(trigger()).toBeDisabled();
		fireEvent.click(trigger());
		expect(
			screen.queryByRole("button", { name: "Discovered model" }),
		).toBeNull();
		expect(invalidate).not.toHaveBeenCalled();
	});

	it("uses discovered IDs, waits for ACK and refreshes only this platform's shell snapshots", async () => {
		let ack: (() => void) | undefined;
		const select = vi.fn(
			() =>
				new Promise<void>((resolve) => {
					ack = resolve;
				}),
		);
		const { trigger, invalidate, rerender } = setup(select);
		fireEvent.click(trigger());
		expect(screen.getByRole("button", { name: "Saved model" })).toHaveAttribute(
			"aria-pressed",
			"true",
		);
		fireEvent.click(screen.getByRole("button", { name: "Discovered model" }));
		await waitFor(() =>
			expect(select).toHaveBeenCalledWith("native-session", "discovered"),
		);
		expect(trigger()).toBeDisabled();
		expect(
			screen.getByRole("button", { name: "Discovered model" }),
		).toBeDisabled();
		expect(trigger()).toHaveTextContent("Saved model");
		expect(invalidate).not.toHaveBeenCalled();
		await act(async () => ack?.());
		await waitFor(() => expect(invalidate).toHaveBeenCalledTimes(2));
		expect(invalidate).toHaveBeenCalledWith({
			queryKey: ["oqto-ui", "test-platform"],
		});
		expect(invalidate).toHaveBeenCalledWith({
			queryKey: ["oqto-ui-compositor", "test-platform"],
		});
		expect(trigger()).toHaveTextContent("Saved model");
		rerender({ ...session, model: "discovered" });
		expect(trigger()).toHaveTextContent("Discovered model");
		rerender({ ...session, model: "external-model" });
		expect(trigger()).toHaveTextContent("external-model");
	});

	it("reports native rejection without changing selection or invalidating snapshots, then allows retry", async () => {
		const select = vi
			.fn()
			.mockRejectedValueOnce(new Error("Session is active"))
			.mockResolvedValueOnce(undefined);
		const { trigger, invalidate } = setup(select);
		fireEvent.click(trigger());
		fireEvent.click(screen.getByRole("button", { name: "Discovered model" }));
		expect(await screen.findByRole("alert")).toHaveTextContent(
			"Session is active",
		);
		expect(trigger()).toHaveTextContent("Saved model");
		expect(invalidate).not.toHaveBeenCalled();
		expect(trigger()).toBeEnabled();
		fireEvent.click(screen.getByRole("button", { name: "Discovered model" }));
		await waitFor(() => expect(invalidate).toHaveBeenCalledTimes(2));
		expect(screen.queryByRole("alert")).toBeNull();
	});
});
