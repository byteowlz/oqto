import {
	cleanup,
	fireEvent,
	render,
	screen,
	waitFor,
} from "@testing-library/react";
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import { i18n, initI18n } from "../lib/i18n";
beforeAll(async () => {
	await initI18n();
	await i18n.changeLanguage("en");
});
import {
	type LoginCommand,
	MachineProviders,
	parseLoginAttempt,
	safeLoginUrl,
} from "../src/routes/app-shell/MachineProviders";
afterEach(cleanup);
const catalog = [
	{ id: "openai-codex", name: "Codex", configured: false, methods: ["oauth"] },
];
const snapshot = {
	provider: "openai-codex",
	id: "attempt-a",
	state: "running",
	events: [
		{ id: "event-a", type: "auth_url", url: "https://example.test/login" },
	],
	prompt: { id: "prompt-a", type: "manual_code", message: "Paste code" },
};

describe("machine-scoped provider authentication", () => {
	it("filters providers locally by name or ID", async () => {
		const call = vi.fn(async () => [
			...catalog,
			{
				id: "anthropic",
				name: "Anthropic",
				configured: false,
				methods: ["oauth"],
			},
		]);
		render(<MachineProviders label="Mac" port={{ call }} close={() => {}} />);
		await screen.findByText("Anthropic");
		fireEvent.change(screen.getByLabelText("Find a provider"), {
			target: { value: "CODEX" },
		});
		expect(screen.getByText("Codex")).toBeVisible();
		expect(screen.queryByText("Anthropic")).toBeNull();
		expect(call).toHaveBeenCalledTimes(1);
	});
	it("rejects unsafe authentication links and unknown response states", () => {
		expect(safeLoginUrl("javascript:alert(1)")).toBeUndefined();
		expect(safeLoginUrl("https://user:password@example.test")).toBeUndefined();
		expect(safeLoginUrl("http://localhost:1234")).toBeUndefined();
		expect(parseLoginAttempt(snapshot).prompt?.id).toBe("prompt-a");
		expect(() =>
			parseLoginAttempt({ ...snapshot, state: "invented" }),
		).toThrow();
	});
	it("uses dedicated private input, never issues browser commit, and cancels on unmount", async () => {
		const call = vi.fn(async (command: LoginCommand) =>
			command.command === "providers"
				? catalog
				: command.command === "answer"
					? { ...snapshot, prompt: null }
					: snapshot,
		);
		const view = render(
			<MachineProviders label="Mac" port={{ call }} close={() => {}} />,
		);
		fireEvent.click(await screen.findByText("Sign in"));
		const input = await screen.findByLabelText("Paste code");
		expect(input).toHaveAttribute("type", "password");
		expect(input).toHaveAttribute("autocomplete", "off");
		const link = screen.getByText("Open provider sign-in");
		expect(link).toHaveAttribute("rel", "noopener noreferrer");
		fireEvent.change(input, { target: { value: "private-answer" } });
		fireEvent.click(screen.getByText("Continue"));
		await waitFor(() =>
			expect(call).toHaveBeenCalledWith(
				{
					command: "answer",
					attempt: "attempt-a",
					prompt: "prompt-a",
					value: "private-answer",
				},
				expect.any(AbortSignal),
			),
		);
		await waitFor(() =>
			expect(screen.queryByDisplayValue("private-answer")).toBeNull(),
		);
		expect(
			call.mock.calls.some(([command]) => String(command.command) === "commit"),
		).toBe(false);
		view.unmount();
		expect(call).toHaveBeenCalledWith({
			command: "cancel",
			attempt: "attempt-a",
		});
	});
	it("does not render raw transport errors or provider secrets", async () => {
		const call = vi.fn(async () => {
			throw new Error("private-token-from-server");
		});
		render(<MachineProviders label="Mac" port={{ call }} close={() => {}} />);
		expect(await screen.findByRole("alert")).not.toHaveTextContent(
			"private-token-from-server",
		);
	});
	it("polls owning attempt and renders saved credentials without repeating login", async () => {
		const call = vi.fn(async (command: LoginCommand) =>
			command.command === "providers"
				? catalog
				: command.command === "status"
					? { ...snapshot, state: "saved", events: [], prompt: null }
					: snapshot,
		);
		render(<MachineProviders label="Mac" port={{ call }} close={() => {}} />);
		fireEvent.click(await screen.findByText("Sign in"));
		expect(
			await screen.findByText(
				"Credentials saved on this machine.",
				{},
				{ timeout: 2500 },
			),
		).toBeVisible();
		expect(
			call.mock.calls.filter(([command]) => command.command === "start"),
		).toHaveLength(1);
	});
});
