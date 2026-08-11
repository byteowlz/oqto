import { render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { App } from "../src/App";

vi.mock("@/hooks/use-auth", () => ({
	useCurrentUser: () => ({ data: null, isLoading: false, isFetched: true }),
	useLogout: vi.fn(),
	useInvalidateAuth: vi.fn(),
}));

vi.mock("../src/routes/LoginPage", () => ({
	LoginPage: () => <h1>Login required</h1>,
}));

afterEach(() => {
	window.history.replaceState({}, "", "/");
});

describe("Workbench Lab authentication", () => {
	it("redirects an unauthenticated lab request to login with its full return URL", async () => {
		window.history.replaceState(
			{},
			"",
			"/workbench-lab?workDirectory=oqto&session=frontend-rebuild",
		);

		render(<App />);

		await waitFor(() => {
			expect(window.location.pathname).toBe("/login");
		});
		expect(window.location.search).toBe(
			"?redirect=%2Fworkbench-lab%3FworkDirectory%3Doqto%26session%3Dfrontend-rebuild",
		);
		expect(
			screen.getByRole("heading", { name: "Login required" }),
		).toBeInTheDocument();
	});
});
