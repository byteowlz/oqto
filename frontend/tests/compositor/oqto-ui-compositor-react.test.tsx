import type { ContentRef } from "@/src/oqto-ui/compositor/index";
import { CompositorHost } from "@/src/oqto-ui/compositor/react/CompositorHost";
import type { ContentRenderContext } from "@/src/oqto-ui/compositor/react/contracts";
import { createCompositorStore } from "@/src/oqto-ui/compositor/react/store";
import { act, fireEvent, render } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import {
	LABELS,
	VIEWPORT,
	activeArrangementOf,
	chatContent,
	classicLayout,
	containerByRole,
	filesContent,
	gitContent,
	sessionsContent,
	terminalContent,
} from "./fixtures";

const DESKTOP = { ...VIEWPORT, responsive: "scroll" as const };

function probeSetup(viewport = DESKTOP) {
	const store = createCompositorStore(classicLayout());
	const renderCounts = new Map<string, number>();
	const renderContent = (
		content: ContentRef,
		context: ContentRenderContext,
	) => {
		renderCounts.set(content.id, (renderCounts.get(content.id) ?? 0) + 1);
		return (
			<output
				data-testid={`content-${content.id}`}
				data-focused={context.focused}
			>
				{content.id}
			</output>
		);
	};
	const contentLabel = (content: ContentRef) => `label:${content.id}`;
	const view = render(
		<CompositorHost
			store={store}
			viewport={viewport}
			renderContent={renderContent}
			contentLabel={contentLabel}
			labels={LABELS}
		/>,
	);
	return { store, renderCounts, view };
}

describe("CompositorHost React adapter (v2)", () => {
	it("projects solved tracks onto CSS Grid custom properties", () => {
		const { view } = probeSetup();
		const grid = view.container.querySelector(
			".oqto-compositor-grid",
		) as HTMLElement;
		expect(grid.style.getPropertyValue("--oqto-compositor-columns")).toMatch(
			/^320px [0-9.]+px [0-9.]+px$/,
		);
		expect(grid.style.getPropertyValue("--oqto-compositor-scroll")).toBe("0px");
		const cell = view.container.querySelector(
			".oqto-compositor-cell",
		) as HTMLElement;
		expect(cell.style.getPropertyValue("--oqto-cell-column")).toBe(
			"1 / span 1",
		);
		expect(view.queryAllByRole("tablist")).toHaveLength(0);
		expect(view.getByTestId(`content-${chatContent.id}`)).toBeInTheDocument();
	});

	it("local layout changes do not rerender unrelated Content", () => {
		const { store, renderCounts } = probeSetup();
		const before = new Map(renderCounts);
		act(() => {
			store.commit([
				{ type: "open", content: gitContent, target: { role: "auxiliary" } },
			]);
		});
		expect(renderCounts.get(sessionsContent.id)).toBe(
			before.get(sessionsContent.id),
		);
		expect(renderCounts.get(chatContent.id)).toBe(before.get(chatContent.id));
		expect(renderCounts.get(gitContent.id)).toBe(1);
	});

	it("tab strips activate on click and close buttons dispatch semantic close", () => {
		const { store, view, renderCounts } = probeSetup();
		act(() => {
			store.commit([
				{ type: "open", content: gitContent, target: { role: "auxiliary" } },
			]);
		});
		const tabs = view.getAllByRole("tab");
		expect(tabs.map((tab) => tab.textContent)).toEqual([
			`label:${filesContent.id}`,
			`label:${gitContent.id}`,
		]);
		const before = new Map(renderCounts);
		fireEvent.click(tabs[0]);
		expect(
			containerByRole(store.getSnapshot(), "auxiliary").activeContentId,
		).toBe(filesContent.id);
		expect(renderCounts.get(chatContent.id)).toBe(before.get(chatContent.id));
		fireEvent.click(view.getAllByLabelText(LABELS.closeTab)[1]);
		expect(
			containerByRole(store.getSnapshot(), "auxiliary").stack.map((c) => c.id),
		).toEqual([filesContent.id]);
		expect(store.getSnapshot().revision).toBe(3);
	});

	it("collapse hides the Container without rerendering the other regions", () => {
		const { store, view, renderCounts } = probeSetup();
		const navigationId = containerByRole(store.getSnapshot(), "navigation").id;
		const before = new Map(renderCounts);
		act(() => {
			store.commit([
				{ type: "collapse", containerId: navigationId, collapsed: true },
			]);
		});
		expect(
			view.container.querySelector(`[data-container-id="${navigationId}"]`),
		).toHaveAttribute("hidden");
		expect(renderCounts.get(chatContent.id)).toBe(before.get(chatContent.id));
		expect(renderCounts.get(filesContent.id)).toBe(before.get(filesContent.id));
	});

	it("midsize viewports render the merged projection while the store stays canonical", () => {
		const { store, view } = probeSetup({
			inlineSize: 600,
			blockSize: 800,
			responsive: "merge",
		});
		expect(view.container.querySelector(".oqto-compositor")).toHaveAttribute(
			"data-merges",
			"1",
		);
		expect(view.getAllByRole("tab")).toHaveLength(2);
		expect(store.getSnapshot().containers).toHaveLength(3);
	});

	it("renders a flush Lane outside the scrolled body", () => {
		const { store, view } = probeSetup();
		act(() => {
			store.commit([
				{
					type: "open",
					content: terminalContent,
					target: { role: "auxiliary" },
				},
				{ type: "split", contentId: terminalContent.id, edge: "block-end" },
			]);
			store.commit([
				{
					type: "flush",
					rowId: activeArrangementOf(store.getSnapshot()).grid.rows[1].id,
					flush: true,
				},
			]);
		});
		const lane = view.container.querySelector(
			".oqto-compositor-lane",
		) as HTMLElement;
		expect(lane).not.toBeNull();
		expect(lane.style.getPropertyValue("--oqto-lane-size")).toBe("240px");
		expect(
			lane.querySelector(`[data-testid="content-${terminalContent.id}"]`),
		).not.toBeNull();
	});

	it("reveal changes visibility, not keyboard focus; stale transactions conflict", () => {
		const { store } = probeSetup();
		const focusBefore = store.getSnapshot().focusedContentId;
		const staleRevision = store.getSnapshot().revision;
		act(() => {
			store.commit([{ type: "reveal", contentId: filesContent.id }]);
		});
		expect(store.getSnapshot().focusedContentId).toBe(focusBefore);
		let result: ReturnType<typeof store.dispatch> | null = null;
		act(() => {
			result = store.dispatch({
				expectedRevision: staleRevision,
				commands: [{ type: "close", contentId: chatContent.id }],
			});
		});
		expect(result?.ok).toBe(false);
		expect(
			containerByRole(store.getSnapshot(), "primary").stack.map((c) => c.id),
		).toEqual([chatContent.id]);
	});
});
