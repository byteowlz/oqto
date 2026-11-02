import {
	fireEvent,
	render as renderView,
	screen,
	within,
} from "@testing-library/react";
import { createInstance } from "i18next";
import type { ReactElement } from "react";
import { I18nextProvider, initReactI18next } from "react-i18next";
import { describe, expect, it } from "vitest";
import { ResourceMockup } from "../../src/oqto-ui/dev/policy-editor/ResourceMockup";

const i18n = createInstance();
void i18n
	.use(initReactI18next)
	.init({ lng: "en", resources: {}, interpolation: { escapeValue: false } });
const render = (element: ReactElement) =>
	renderView(<I18nextProvider i18n={i18n}>{element}</I18nextProvider>);

const node = (name: string, kind: string) =>
	screen.getByRole("button", { name: `${name}, ${kind}` });
const operation = (label: string, resource: string) =>
	screen.getByRole("group", { name: `${label} for ${resource}` });

describe("resource-access mockup", () => {
	it("navigates a realistic deployment without exposing edit controls", () => {
		render(<ResourceMockup />);
		expect(screen.getAllByRole("button", { name: /, workdir$/ })).toHaveLength(
			16,
		);
		expect(
			screen.getAllByRole("button", { name: /, workspace$/ }),
		).toHaveLength(4);
		expect(screen.getAllByRole("button", { name: /, service$/ })).toHaveLength(
			6,
		);
		expect(
			screen.getAllByRole("button", { name: "Engineering, wiki" }),
		).toHaveLength(1);
		expect(node("You", "account")).toBeInTheDocument();
		fireEvent.click(node("Web app", "workdir"));
		expect(screen.getByText("FOCUSED WORKDIR").nextSibling).toHaveTextContent(
			"Web app",
		);
		fireEvent.click(node("Engineering", "wiki"));
		expect(
			screen.queryByRole("group", { name: /for Engineering$/ }),
		).not.toBeInTheDocument();
		expect(
			screen.getByRole("button", { name: "Apply unavailable" }),
		).toBeDisabled();
	});

	it("edits access on the node itself while the subject stays pinned", () => {
		render(<ResourceMockup />);
		fireEvent.click(screen.getByRole("button", { name: "Edit access" }));
		expect(operation("Execute", "Oqto")).toBeInTheDocument();
		fireEvent.click(node("Credential broker · brkr", "service"));
		fireEvent.click(
			within(operation("Use", "Credential broker · brkr")).getByRole("button", {
				name: "Allow",
			}),
		);
		expect(screen.getByText("1 proposed for Oqto")).toBeInTheDocument();
		fireEvent.click(node("Engineering", "wiki"));
		fireEvent.click(
			within(operation("Search / read", "Engineering")).getByRole("button", {
				name: "Ask",
			}),
		);
		expect(screen.getByText("2 proposed for Oqto")).toBeInTheDocument();
		fireEvent.click(node("Web app", "workdir"));
		expect(screen.getByText(/Subject is pinned/)).toBeInTheDocument();
		expect(
			screen.getByText("EDITING ACCESS FOR").nextSibling,
		).toHaveTextContent("Oqto");
		fireEvent.click(screen.getByRole("button", { name: "Navigate" }));
		fireEvent.click(node("Web app", "workdir"));
		expect(screen.getByText("No proposed changes")).toBeInTheDocument();
		expect(
			screen.getByText("+ drafts for 1 other workdirs"),
		).toBeInTheDocument();
		fireEvent.click(node("Oqto", "workdir"));
		expect(screen.getByText("2 proposed for Oqto")).toBeInTheDocument();
	});

	it("keeps ceilings locked and drops no-op proposals", () => {
		render(<ResourceMockup />);
		fireEvent.click(node("Customer portal", "workdir"));
		fireEvent.click(screen.getByRole("button", { name: "Edit access" }));
		fireEvent.click(node("Engineering", "wiki"));
		expect(operation("Search / read", "Engineering")).toBeDisabled();
		fireEvent.click(node("Client confidential", "wiki"));
		expect(operation("Manage", "Client confidential")).toBeDisabled();
		const read = operation("Search / read", "Client confidential");
		fireEvent.click(within(read).getByRole("button", { name: "Deny" }));
		expect(
			screen.getByText("1 proposed for Customer portal"),
		).toBeInTheDocument();
		fireEvent.click(within(read).getByRole("button", { name: "Allow" }));
		expect(screen.getByText("No proposed changes")).toBeInTheDocument();
	});

	it("switches layout direction without losing state", () => {
		render(<ResourceMockup />);
		const down = screen.getByRole("button", { name: "Top down" });
		fireEvent.click(down);
		expect(down).toHaveAttribute("aria-pressed", "true");
		expect(screen.getAllByRole("button", { name: /, workdir$/ })).toHaveLength(
			16,
		);
		fireEvent.click(screen.getByRole("button", { name: "Edit access" }));
		expect(operation("Execute", "Oqto")).toBeInTheDocument();
		fireEvent.click(screen.getByRole("button", { name: "Left to right" }));
		expect(screen.getAllByRole("button", { name: /, service$/ })).toHaveLength(
			6,
		);
	});
});
