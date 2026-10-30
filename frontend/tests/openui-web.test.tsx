import { readFileSync } from "node:fs";
import { createRequire } from "node:module";
import { dirname, join } from "node:path";
import { OpenUIPart, readOpenUIPart } from "@/src/oqto-ui/chat/OpenUIPart";
import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { initI18n } from "../lib/i18n";

initI18n();

const fixtures = join(
	dirname(
		createRequire(import.meta.url).resolve(
			"@byteowlz/oqto-openui/schema/profile.json",
		),
	),
	"..",
	"fixtures",
);
const fixture = (name: string) =>
	readFileSync(join(fixtures, name, "input.oui"), "utf8");

describe("openui tool input", () => {
	it("reads only a program, an optional fallback and wait", () => {
		expect(
			readOpenUIPart({ program: "root = Card([])", fallback: "x", wait: true }),
		).toEqual({
			program: "root = Card([])",
			fallback: "x",
			wait: true,
		});
		expect(readOpenUIPart({ program: "   " })).toBeNull();
		expect(readOpenUIPart({ fallback: "x" })).toBeNull();
		expect(readOpenUIPart(undefined)).toBeNull();
	});
});

describe("openui web rendering", () => {
	it("renders a dashboard program with cards, KPIs and tables", () => {
		const { container } = render(
			<OpenUIPart input={{ program: fixture("dashboard") }} />,
		);
		expect(container.querySelector(".wb-openui")).not.toBeNull();
		expect(screen.getByText("oqto · week 43")).toBeInTheDocument();
		expect(screen.getByText("1,284")).toBeInTheDocument();
		expect(container.querySelector(".wb-openui__table")).not.toBeNull();
		// Tabs are disclosures in the web host; the first one opens.
		expect(
			container.querySelectorAll(".wb-openui__tabs details").length,
		).toBeGreaterThan(1);
	});

	it("renders a plan with steps and inert buttons, never an action", () => {
		const { container } = render(
			<OpenUIPart input={{ program: fixture("plan-approve"), wait: true }} />,
		);
		expect(screen.getByText("Migrate pairing store")).toBeInTheDocument();
		expect(container.querySelectorAll(".wb-openui__steps li").length).toBe(4);
		const buttons = [
			...container.querySelectorAll<HTMLButtonElement>(
				".wb-openui__buttons button",
			),
		];
		expect(buttons.map((b) => b.textContent)).toEqual([
			"Approve plan",
			"Not now",
		]);
		for (const button of buttons) expect(button.disabled).toBe(true);
		expect(
			screen.getByText(/answers in the session that asked/i),
		).toBeInTheDocument();
	});

	it("shows the answer the agent tool reported, not one invented here", () => {
		render(
			<OpenUIPart
				input={{ program: fixture("plan-approve"), wait: true }}
				answered={{ approved: true }}
			/>,
		);
		expect(screen.getByText(/Answered in the terminal/)).toBeInTheDocument();
		expect(screen.getByText(/approved/)).toBeInTheDocument();
	});

	it("falls back to the text when the program cannot render, and never injects HTML", () => {
		const { container } = render(
			<OpenUIPart
				input={{
					program: "not openui at all",
					fallback: "<img src=x onerror=alert(1)>",
				}}
			/>,
		);
		expect(container.querySelector("img")).toBeNull();
		expect(
			screen.getByText("<img src=x onerror=alert(1)>"),
		).toBeInTheDocument();
		expect(container.querySelector(".wb-openui--fallback")).not.toBeNull();
	});

	it("drops unknown components but keeps the rest", () => {
		render(<OpenUIPart input={{ program: fixture("invalid-unknown") }} />);
		expect(screen.getByText("still renders")).toBeInTheDocument();
	});

	it("renders an editable table read-only, never as an editor", () => {
		const { container } = render(
			<OpenUIPart input={{ program: fixture("editable-table") }} />,
		);
		const rows = container.querySelectorAll(".wb-openui__table tbody tr");
		expect(rows.length).toBe(3);
		expect(container.querySelector("input")).toBeNull();
		expect(screen.getByText("Owner")).toBeInTheDocument();
		expect(screen.getByText("Qualify Mac runner")).toBeInTheDocument();
	});

	it("marks components the web host does not draw yet instead of hiding them silently", () => {
		const program = `root = Stack([c])
c = Card([g])
g = ImageGallery([{src: "https://example.invalid/a.png", alt: "A"}])`;
		render(<OpenUIPart input={{ program }} />);
		expect(
			screen.getByText(/ImageGallery is not rendered in the web chat yet/),
		).toBeInTheDocument();
	});
});
