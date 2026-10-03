import { act, fireEvent, render, screen, within } from "@testing-library/react";
import { createInstance } from "i18next";
import { I18nextProvider } from "react-i18next";
import { beforeAll, describe, expect, it } from "vitest";
import de from "../messages/de.json";
import en from "../messages/en.json";
import { SelectionView } from "../src/oqto-ui/chat/SelectionView";
import { review, selectionHarness, settle } from "./selection-fixtures";
const i18n = createInstance();
beforeAll(async () => {
	await i18n.init({
		lng: "en",
		fallbackLng: "en",
		resources: { en: { translation: en }, de: { translation: de } },
		interpolation: { escapeValue: false },
	});
});
async function mount(mode: "questions" | "review" = "questions") {
	const h = selectionHarness(review(mode));
	const host = { session: () => h.controller };
	const result = render(
		<I18nextProvider i18n={i18n}>
			<SelectionView host={host} sessionId="platform-session" />
		</I18nextProvider>,
	);
	await act(settle);
	act(() => h.present());
	await act(settle);
	return { ...h, ...result };
}
describe("first-party selection user flows", () => {
	it("collects single/multi/Other/text/notes and saves without submission", async () => {
		const h = await mount();
		expect(screen.getByLabelText("Small · Recommended")).not.toBeChecked();
		fireEvent.click(screen.getByLabelText("Small · Recommended"));
		fireEvent.change(screen.getByLabelText("Reason"), {
			target: { value: "Why" },
		});
		fireEvent.click(screen.getByLabelText("Alpha"));
		fireEvent.click(screen.getByLabelText("Beta"));
		fireEvent.change(screen.getByLabelText("Other"), {
			target: { value: "Different" },
		});
		fireEvent.change(screen.getByLabelText("Answer"), {
			target: { value: "Summary\nwith lines" },
		});
		await act(async () => {
			fireEvent.click(screen.getByRole("button", { name: "Save draft" }));
			await settle();
		});
		expect(h.record().state).toBe("draft");
		expect(h.record().answers.direction).toEqual({
			answered: true,
			selectedIds: [],
			text: "Different",
			note: "Why",
		});
		expect(h.record().answers.features.selectedIds).toEqual(["a", "b"]);
		await act(async () => {
			fireEvent.click(screen.getByRole("button", { name: "Submit" }));
			await settle();
		});
		expect(h.record().state).toBe("submitted");
		expect(screen.getByRole("button", { name: "Save draft" })).toBeDisabled();
	});
	it("distinguishes confirm-none/skipped/unsure/unanswered; cancellation is explicit", async () => {
		const h = await mount();
		fireEvent.click(screen.getByRole("button", { name: "Confirm none" }));
		const direction = screen.getByRole("group", { name: "Direction" });
		fireEvent.click(within(direction).getByRole("button", { name: "Skipped" }));
		expect(h.controller.snapshot().answers.direction.disposition).toBe(
			"skipped",
		);
		fireEvent.click(within(direction).getByRole("button", { name: "Unsure" }));
		expect(h.controller.snapshot().answers.direction.disposition).toBe(
			"unsure",
		);
		fireEvent.click(
			within(direction).getByRole("button", { name: "Reset answer" }),
		);
		expect(h.controller.snapshot().answers.direction.answered).toBe(false);
		expect(h.controller.snapshot().answers.features).toMatchObject({
			answered: true,
			selectedIds: [],
		});
		fireEvent.click(screen.getByRole("button", { name: "Cancel review" }));
		expect(h.record().state).toBe("draft");
		fireEvent.click(screen.getByRole("button", { name: "Keep editing" }));
		expect(h.record().state).toBe("draft");
		fireEvent.click(screen.getByRole("button", { name: "Cancel review" }));
		await act(async () => {
			fireEvent.click(screen.getByRole("button", { name: "Confirm" }));
			await settle();
		});
		expect(h.record().state).toBe("cancelled");
	});
	it("searches and filters grouped Review without changing IDs/answers", async () => {
		const h = await mount("review");
		fireEvent.change(screen.getByRole("searchbox", { name: "Search review" }), {
			target: { value: "pear" },
		});
		expect(
			screen.queryByRole("group", { name: "First · Apple" }),
		).not.toBeInTheDocument();
		fireEvent.click(screen.getByLabelText("Keep"));
		expect(h.controller.snapshot().answers.pear.selectedIds).toEqual(["keep"]);
		fireEvent.change(screen.getByRole("searchbox"), { target: { value: "" } });
		fireEvent.change(screen.getByRole("combobox", { name: "Group" }), {
			target: { value: "one" },
		});
		expect(
			screen.getByRole("group", { name: "First · Apple" }),
		).toBeInTheDocument();
		expect(
			screen.queryByRole("group", { name: "Second · Pear" }),
		).not.toBeInTheDocument();
		expect(h.controller.snapshot().answers.pear.selectedIds).toEqual(["keep"]);
	});
	it("reports validation and CAS errors, preserves edits, confirms destructive reload", async () => {
		const h = await mount();
		await act(async () => {
			fireEvent.click(screen.getByRole("button", { name: "Submit" }));
			await settle();
		});
		expect(screen.getByRole("alert")).toHaveTextContent(
			"Required answer missing",
		);
		fireEvent.change(screen.getByLabelText("Answer"), {
			target: { value: "Local" },
		});
		h.serverRecord({ ...h.record(), revision: 5 });
		await act(async () => {
			fireEvent.click(screen.getByRole("button", { name: "Save draft" }));
			await settle();
		});
		expect(screen.getByRole("alert")).toHaveTextContent("Revision conflict");
		fireEvent.click(screen.getByRole("button", { name: "Reload" }));
		expect(screen.getByLabelText("Answer")).toHaveValue("Local");
		await act(async () => {
			fireEvent.click(screen.getByRole("button", { name: "Confirm" }));
			await settle();
		});
		expect(screen.getByLabelText("Answer")).toHaveValue("");
		expect(h.controller.snapshot().record?.revision).toBe(5);
	});
	it("keeps notes separate from decisions and allows combined multi plus Other", async () => {
		const initial = review();
		initial.spec.questions[1].allowOther = true;
		const h = selectionHarness(initial);
		const host = { session: () => h.controller };
		render(
			<I18nextProvider i18n={i18n}>
				<SelectionView host={host} sessionId="s" />
			</I18nextProvider>,
		);
		await act(settle);
		act(() => h.present());
		fireEvent.change(screen.getByLabelText("Reason"), {
			target: { value: "Only a note" },
		});
		expect(h.controller.snapshot().answers.direction.answered).toBe(false);
		const features = within(screen.getByRole("group", { name: "Features" }));
		fireEvent.change(features.getByLabelText("Other"), {
			target: { value: "Gamma" },
		});
		fireEvent.click(features.getByLabelText("Alpha"));
		expect(h.controller.snapshot().answers.features).toMatchObject({
			answered: true,
			text: "Gamma",
			selectedIds: ["a"],
		});
	});
	it("does not treat unchecking the last choice as confirm-none", async () => {
		const h = await mount();
		fireEvent.click(screen.getByLabelText("Alpha"));
		fireEvent.click(screen.getByLabelText("Alpha"));
		expect(h.controller.snapshot().answers.features.answered).toBe(false);
		fireEvent.click(screen.getByRole("button", { name: "Confirm none" }));
		expect(h.controller.snapshot().answers.features.answered).toBe(true);
	});
	it("renders unknown schemas as explicit recoverable errors rather than agent code", async () => {
		const h = selectionHarness();
		const host = { session: () => h.controller };
		render(
			<I18nextProvider i18n={i18n}>
				<SelectionView host={host} sessionId="s" />
			</I18nextProvider>,
		);
		await act(settle);
		act(() =>
			h.present({
				version: 1,
				bindingIds: ["connection-binding"],
				review: { ...review(), spec: { version: 2, mode: "html" } },
			}),
		);
		expect(screen.getByRole("alert")).toBeVisible();
		expect(screen.getByRole("button", { name: "Reload" })).toBeVisible();
		expect(document.querySelector("iframe")).toBeNull();
	});
	it("bounds dense inventories to 25 rendered items and retains edits across pages", async () => {
		const initial = review("review");
		initial.spec.questions = Array.from({ length: 60 }, (_, index) => ({
			...initial.spec.questions[0],
			id: `row${index}`,
			title: `Row ${index}`,
		}));
		initial.answers = {};
		const h = selectionHarness(initial);
		const host = { session: () => h.controller };
		render(
			<I18nextProvider i18n={i18n}>
				<SelectionView host={host} sessionId="s" />
			</I18nextProvider>,
		);
		await act(settle);
		act(() => h.present());
		expect(screen.getAllByRole("group")).toHaveLength(25);
		fireEvent.click(
			within(screen.getAllByRole("group")[0]).getByLabelText("Keep"),
		);
		fireEvent.click(screen.getByRole("button", { name: "Next" }));
		expect(screen.getAllByRole("group")).toHaveLength(25);
		fireEvent.click(screen.getByRole("button", { name: "Previous" }));
		expect(
			within(screen.getAllByRole("group")[0]).getByLabelText("Keep"),
		).toBeChecked();
	});
	it("uses unique control identity for simultaneous views and one domain state", async () => {
		const h = selectionHarness();
		const host = { session: () => h.controller };
		render(
			<I18nextProvider i18n={i18n}>
				<SelectionView host={host} sessionId="s" />
				<SelectionView host={host} sessionId="s" />
			</I18nextProvider>,
		);
		await act(settle);
		act(() => h.present());
		const answers = screen.getAllByLabelText("Answer");
		expect(answers[0].id).not.toBe(answers[1].id);
		fireEvent.change(answers[0], { target: { value: "Shared" } });
		expect(answers[1]).toHaveValue("Shared");
	});
});
