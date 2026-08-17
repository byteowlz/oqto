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

describe("OqtoUI authentication", () => {
	it("preserves the public Session return URL during login redirect", async () => {
		window.history.replaceState({}, "", "/oqto-ui?session=oqto-public-1");
		render(<App />);
		await waitFor(() => expect(window.location.pathname).toBe("/login"));
		expect(window.location.search).toBe(
			"?redirect=%2Foqto-ui%3Fsession%3Doqto-public-1",
		);
		expect(
			screen.getByRole("heading", { name: "Login required" }),
		).toBeInTheDocument();
	});
});
