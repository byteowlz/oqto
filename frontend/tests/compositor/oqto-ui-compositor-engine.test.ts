import {
	type ContentRef,
	type LayoutCommand,
	type LayoutSnapshot,
	applyTransaction,
	contentIdFrom,
} from "@/src/oqto-ui/compositor/index";
import { checkLayoutInvariants } from "@/src/oqto-ui/compositor/kernel/invariants";
import { createEmptyLayout } from "@/src/oqto-ui/compositor/kernel/preset";
import { describe, expect, it } from "vitest";
import {
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
import { deepFreeze } from "./harness";

function commit(
	snapshot: LayoutSnapshot,
	...commands: LayoutCommand[]
): LayoutSnapshot {
	const result = applyTransaction(deepFreeze(snapshot), {
		expectedRevision: snapshot.revision,
		commands,
		viewport: VIEWPORT,
	});
	if (!result.ok)
		throw new Error(
			`expected success, got ${JSON.stringify(result.rejection)}`,
		);
	expect(checkLayoutInvariants(result.snapshot)).toEqual([]);
	return result.snapshot;
}

function reject(snapshot: LayoutSnapshot, ...commands: LayoutCommand[]) {
	const result = applyTransaction(deepFreeze(snapshot), {
		expectedRevision: snapshot.revision,
		commands,
	});
	if (result.ok) throw new Error("expected rejection, got success");
	return result.rejection;
}

function events(snapshot: LayoutSnapshot, ...commands: LayoutCommand[]) {
	const result = applyTransaction(snapshot, {
		expectedRevision: snapshot.revision,
		commands,
		viewport: VIEWPORT,
	});
	if (!result.ok)
		throw new Error(
			`expected success, got ${JSON.stringify(result.rejection)}`,
		);
	return result.events;
}

describe("layout engine transactions", () => {
	it("rejects stale revisions with a typed conflict", () => {
		const result = applyTransaction(classicLayout(), {
			expectedRevision: 7,
			commands: [{ type: "focus", contentId: filesContent.id }],
		});
		expect(result.ok).toBe(false);
		if (!result.ok)
			expect(result.rejection).toEqual({
				reason: "revision-conflict",
				expected: 7,
				actual: 0,
			});
	});

	it("bumps the revision exactly once per accepted transaction", () => {
		const layout = classicLayout();
		const next = commit(
			layout,
			{ type: "focus", contentId: filesContent.id },
			{
				type: "collapse",
				containerId: containerByRole(layout, "navigation").id,
				collapsed: true,
			},
		);
		expect(next.revision).toBe(1);
	});

	it("applies multi-command transactions atomically", () => {
		const layout = deepFreeze(classicLayout());
		const rejection = reject(
			layout,
			{
				type: "collapse",
				containerId: containerByRole(layout, "navigation").id,
				collapsed: true,
			},
			{ type: "reveal", contentId: contentIdFrom("never-placed") },
		);
		expect(rejection.reason).toBe("not-placed");
		expect(containerByRole(layout, "navigation").collapsed).toBe(false);
	});

	it("keeps unchanged Containers and Arrangements referentially identical", () => {
		const layout = classicLayout();
		const nav = containerByRole(layout, "navigation");
		const next = commit(layout, {
			type: "activate",
			contentId: filesContent.id,
		});
		expect(containerByRole(next, "navigation")).toBe(nav);
		expect(activeArrangementOf(next)).toBe(activeArrangementOf(layout));
	});
});

describe("open and reveal", () => {
	it("open is idempotent for a placed Content identity", () => {
		const layout = classicLayout();
		expect(events(layout, { type: "open", content: chatContent })).toEqual([
			{
				type: "revealed",
				contentId: chatContent.id,
				containerId: containerByRole(layout, "primary").id,
			},
		]);
	});

	it("rejects reusing a Content identity with a different kind", () => {
		expect(
			reject(classicLayout(), {
				type: "open",
				content: { id: chatContent.id, kind: "files" },
			}).reason,
		).toBe("invalid-command");
	});

	it("tabs new Content into the targeted role", () => {
		const next = commit(classicLayout(), {
			type: "open",
			content: gitContent,
			target: { role: "auxiliary" },
		});
		const auxiliary = containerByRole(next, "auxiliary");
		expect(auxiliary.stack.map((content) => content.id)).toEqual([
			filesContent.id,
			gitContent.id,
		]);
		expect(auxiliary.activeContentId).toBe(gitContent.id);
	});

	it("rejects open into an unknown container or arrangement", () => {
		expect(
			reject(classicLayout(), {
				type: "open",
				content: gitContent,
				target: { containerId: "container-99" as never },
			}).reason,
		).toBe("unknown-container");
		expect(
			reject(classicLayout(), {
				type: "open",
				content: gitContent,
				target: { arrangementId: "arrangement-9" as never },
			}).reason,
		).toBe("unknown-arrangement");
	});

	it("creates a Container, row, and column when opening into an empty layout", () => {
		const next = commit(createEmptyLayout(), {
			type: "open",
			content: chatContent,
		});
		expect(next.containers).toHaveLength(1);
		expect(activeArrangementOf(next).grid.rows).toHaveLength(1);
		expect(activeArrangementOf(next).grid.columns).toHaveLength(1);
		expect(next.focusedContentId).toBe(chatContent.id);
	});

	it("reveal returns a typed not-placed result and never duplicates", () => {
		expect(
			reject(classicLayout(), {
				type: "reveal",
				contentId: contentIdFrom("chat:not-open"),
			}),
		).toEqual({
			reason: "not-placed",
			contentId: contentIdFrom("chat:not-open"),
		});
	});

	it("reveal restores visibility without stealing focus", () => {
		const layout = classicLayout();
		const auxiliaryId = containerByRole(layout, "auxiliary").id;
		const collapsed = commit(layout, {
			type: "collapse",
			containerId: auxiliaryId,
			collapsed: true,
		});
		const revealed = commit(collapsed, {
			type: "reveal",
			contentId: filesContent.id,
		});
		expect(containerByRole(revealed, "auxiliary").collapsed).toBe(false);
		expect(revealed.focusedContentId).toBe(chatContent.id);
	});

	it("focus targets placed Content without changing visibility", () => {
		const layout = classicLayout();
		const auxiliaryId = containerByRole(layout, "auxiliary").id;
		const collapsed = commit(layout, {
			type: "collapse",
			containerId: auxiliaryId,
			collapsed: true,
		});
		const focused = commit(collapsed, {
			type: "focus",
			contentId: filesContent.id,
		});
		expect(focused.focusedContentId).toBe(filesContent.id);
		expect(containerByRole(focused, "auxiliary").collapsed).toBe(true);
	});
});

describe("move, split, lanes, resize", () => {
	it("moves Content between Containers and applies retain empty behavior", () => {
		const layout = classicLayout();
		const next = commit(layout, {
			type: "move",
			contentId: filesContent.id,
			destination: { containerId: containerByRole(layout, "primary").id },
		});
		expect(containerByRole(next, "primary").stack.map((c) => c.id)).toEqual([
			chatContent.id,
			filesContent.id,
		]);
		expect(containerByRole(next, "auxiliary").stack).toHaveLength(0);
		expect(activeArrangementOf(next).grid.columns).toHaveLength(3);
	});

	it("reorders within a Container without losing identity", () => {
		const layout = commit(classicLayout(), {
			type: "open",
			content: gitContent,
			target: { role: "auxiliary" },
		});
		const next = commit(layout, {
			type: "move",
			contentId: gitContent.id,
			destination: {
				containerId: containerByRole(layout, "auxiliary").id,
				position: 0,
			},
		});
		expect(containerByRole(next, "auxiliary").stack.map((c) => c.id)).toEqual([
			gitContent.id,
			filesContent.id,
		]);
	});

	it("split beside a Container inserts a column and closing repairs the grid", () => {
		const layout = classicLayout();
		const primaryId = containerByRole(layout, "primary").id;
		const split = commit(layout, {
			type: "split",
			contentId: filesContent.id,
			relativeTo: primaryId,
			edge: "inline-end",
		});
		expect(activeArrangementOf(split).grid.columns).toHaveLength(4);
		const created = split.containers.find(
			(c) => c.stack[0]?.id === filesContent.id,
		);
		expect(created?.emptyBehavior).toBe("remove");
		const closed = commit(split, { type: "close", contentId: filesContent.id });
		expect(activeArrangementOf(closed).grid.columns).toHaveLength(3);
		expect(closed.containers).toHaveLength(3);
	});

	it("block split inserts a row confined to the target's columns and grows spanning neighbors", () => {
		const layout = classicLayout();
		const primaryId = containerByRole(layout, "primary").id;
		const next = commit(layout, {
			type: "split",
			contentId: filesContent.id,
			relativeTo: primaryId,
			edge: "block-end",
		});
		const grid = activeArrangementOf(next).grid;
		expect(grid.rows).toHaveLength(2);
		const nav = grid.placements.find(
			(p) => p.containerId === containerByRole(layout, "navigation").id,
		);
		expect(nav?.rowSpan).toBe(2);
		const created = grid.placements.find(
			(p) => p.row === 1 && p.containerId !== nav?.containerId,
		);
		expect(created?.column).toBe(1);
		expect(created?.colSpan).toBe(1);
	});

	it("an edge split at the bottom creates a full-width Lane", () => {
		const layout = commit(classicLayout(), {
			type: "open",
			content: terminalContent,
			target: { role: "auxiliary" },
		});
		const next = commit(layout, {
			type: "split",
			contentId: terminalContent.id,
			edge: "block-end",
		});
		const grid = activeArrangementOf(next).grid;
		expect(grid.rows).toHaveLength(2);
		const lane = grid.placements.find((p) => p.row === 1);
		expect(lane?.colSpan).toBe(3);
		expect(
			grid.placements.filter((p) => p.row === 0).every((p) => p.rowSpan === 1),
		).toBe(true);
	});

	it("an edge split at the top creates a Lane above everything", () => {
		const next = commit(classicLayout(), {
			type: "split",
			contentId: filesContent.id,
			edge: "block-start",
		});
		const grid = activeArrangementOf(next).grid;
		expect(grid.placements.find((p) => p.row === 0)?.colSpan).toBe(
			grid.columns.length,
		);
	});

	it("flush is lane-level, edge-only, and fixed-sized", () => {
		const layout = commit(classicLayout(), {
			type: "split",
			contentId: filesContent.id,
			edge: "block-end",
		});
		const rows = activeArrangementOf(layout).grid.rows;
		const flushed = commit(layout, {
			type: "flush",
			rowId: rows[1].id,
			flush: true,
		});
		expect(activeArrangementOf(flushed).grid.rows[1].flush).toBe(true);
		expect(activeArrangementOf(flushed).grid.rows[1].size.unit).toBe("fixed");
		expect(
			reject(flushed, {
				type: "resize",
				containerId: activeArrangementOf(flushed).grid.placements.find(
					(p) => p.row === 1,
				)?.containerId as never,
				axis: "block",
				size: { unit: "fraction", value: 1 },
			}).reason,
		).toBe("invalid-command");
		const three = commit(flushed, {
			type: "split",
			contentId: chatContent.id,
			edge: "block-start",
		});
		const middle = activeArrangementOf(three).grid.rows[1];
		expect(
			reject(three, { type: "flush", rowId: middle.id, flush: true }).reason,
		).toBe("invalid-command");
	});

	it("rejects a non-positive resize and updates semantic tracks otherwise", () => {
		const layout = classicLayout();
		const primaryId = containerByRole(layout, "primary").id;
		expect(
			reject(layout, {
				type: "resize",
				containerId: primaryId,
				axis: "inline",
				size: { unit: "fraction", value: 0 },
			}).reason,
		).toBe("invalid-command");
		const next = commit(layout, {
			type: "resize",
			containerId: primaryId,
			axis: "inline",
			size: { unit: "fraction", value: 3, min: 400 },
		});
		expect(activeArrangementOf(next).grid.columns[1].size).toEqual({
			unit: "fraction",
			value: 3,
			min: 400,
		});
	});
});

describe("scrolling", () => {
	function wideLayout(): LayoutSnapshot {
		let layout = classicLayout();
		for (const kind of ["a", "b", "c"]) {
			layout = commit(layout, {
				type: "open",
				content: { id: contentIdFrom(`wide:${kind}`), kind },
			});
			layout = commit(layout, {
				type: "split",
				contentId: contentIdFrom(`wide:${kind}`),
				edge: "inline-end",
			});
			const created = layout.containers.find(
				(c) => c.stack[0]?.id === contentIdFrom(`wide:${kind}`),
			);
			layout = commit(layout, {
				type: "resize",
				containerId: created?.id as never,
				axis: "inline",
				size: { unit: "fixed", value: 600 },
			});
		}
		return layout;
	}

	it("reveal of off-screen Content settles the scroll anchor with a viewport", () => {
		const layout = wideLayout();
		const scrolled = commit(layout, {
			type: "reveal",
			contentId: contentIdFrom("wide:c"),
		});
		expect(activeArrangementOf(scrolled).scrollAnchorColumnId).not.toBeNull();
		const back = commit(scrolled, {
			type: "reveal",
			contentId: sessionsContent.id,
		});
		expect(activeArrangementOf(back).scrollAnchorColumnId).toBe(
			activeArrangementOf(back).grid.columns[0].id,
		);
	});

	it("scroll commands are discrete, revisioned, and validated", () => {
		const layout = wideLayout();
		const column = activeArrangementOf(layout).grid.columns[3].id;
		const scrolled = commit(layout, { type: "scroll", anchorColumnId: column });
		expect(activeArrangementOf(scrolled).scrollAnchorColumnId).toBe(column);
		expect(
			reject(layout, { type: "scroll", anchorColumnId: "track-999" as never })
				.reason,
		).toBe("invalid-command");
	});
});

describe("arrangements and workspaces", () => {
	it("creates, switches, and remembers focus per Arrangement", () => {
		const layout = classicLayout();
		const created = commit(layout, {
			type: "arrangement-create",
			label: "second",
			binding: { kind: "work-directory", id: "wd-2" },
			activate: true,
		});
		expect(created.arrangements).toHaveLength(2);
		expect(created.focusedContentId).toBeNull();
		const opened = commit(created, { type: "open", content: gitContent });
		expect(opened.focusedContentId).toBe(gitContent.id);
		const back = commit(opened, {
			type: "arrangement-switch",
			arrangementId: layout.activeArrangementId,
		});
		expect(back.focusedContentId).toBe(chatContent.id);
		const forth = commit(back, {
			type: "arrangement-switch",
			arrangementId: created.activeArrangementId,
		});
		expect(forth.focusedContentId).toBe(gitContent.id);
	});

	it("reveal teleports across Arrangements", () => {
		const layout = commit(
			classicLayout(),
			{ type: "arrangement-create", activate: true },
			{ type: "open", content: gitContent },
			{
				type: "arrangement-switch",
				arrangementId: classicLayout().activeArrangementId,
			},
		);
		const revealed = commit(layout, {
			type: "reveal",
			contentId: gitContent.id,
		});
		expect(revealed.activeArrangementId).toBe(layout.arrangements[1].id);
		expect(revealed.focusedContentId).toBe(gitContent.id);
		expect(
			events(layout, { type: "reveal", contentId: gitContent.id })[0]?.type,
		).toBe("arrangement-switched");
	});

	it("refuses to remove the last Arrangement and closes presentations of removed ones", () => {
		const layout = classicLayout();
		expect(
			reject(layout, {
				type: "arrangement-remove",
				arrangementId: layout.activeArrangementId,
			}).reason,
		).toBe("last-arrangement");
		const two = commit(
			layout,
			{ type: "arrangement-create", activate: true },
			{ type: "open", content: gitContent },
		);
		const removed = commit(two, {
			type: "arrangement-remove",
			arrangementId: two.activeArrangementId,
		});
		expect(removed.arrangements).toHaveLength(1);
		expect(
			removed.containers.some((c) =>
				c.stack.some((content) => content.id === gitContent.id),
			),
		).toBe(false);
		expect(removed.activeArrangementId).toBe(layout.activeArrangementId);
	});

	it("moves Content into another Arrangement without switching", () => {
		const layout = commit(classicLayout(), { type: "arrangement-create" });
		const other = layout.arrangements[1].id;
		const moved = commit(layout, {
			type: "move",
			contentId: filesContent.id,
			destination: { arrangementId: other },
		});
		expect(moved.activeArrangementId).toBe(layout.activeArrangementId);
		expect(moved.arrangements[1].grid.placements).toHaveLength(1);
		expect(moved.focusedContentId).toBe(chatContent.id);
	});

	it("switches the active Workspace as opaque data", () => {
		const next = commit(classicLayout(), {
			type: "workspace-switch",
			workspace: { kind: "workspace", id: "ws-1" },
		});
		expect(next.activeWorkspace).toEqual({ kind: "workspace", id: "ws-1" });
		expect(
			reject(next, {
				type: "workspace-switch",
				workspace: { kind: "workspace", id: "" },
			}).reason,
		).toBe("invalid-command");
	});
});

describe("close, empty behavior, undo", () => {
	it("collapse empty behavior preserves the Container without its track", () => {
		const next = commit(classicLayout(), {
			type: "close",
			contentId: sessionsContent.id,
		});
		expect(containerByRole(next, "navigation").collapsed).toBe(true);
		expect(next.containers).toHaveLength(3);
	});

	it("moves focus deterministically when the focused Content closes", () => {
		const next = commit(classicLayout(), {
			type: "close",
			contentId: chatContent.id,
		});
		expect(next.focusedContentId).toBe(sessionsContent.id);
	});

	it("an emptied Arrangement has no focused target", () => {
		let layout = classicLayout();
		for (const content of [sessionsContent, chatContent, filesContent]) {
			layout = commit(layout, { type: "close", contentId: content.id });
		}
		expect(layout.focusedContentId).toBeNull();
	});

	it("undo restores a prior topology while moving the revision forward", () => {
		const layout = classicLayout();
		const changed = commit(layout, {
			type: "open",
			content: gitContent,
			target: { role: "auxiliary" },
		});
		const undone = commit(changed, { type: "undo", snapshot: layout });
		expect(undone.revision).toBe(2);
		expect(undone.containers).toEqual(layout.containers);
		expect(undone.arrangements).toEqual(layout.arrangements);
	});

	it("rejects undo onto a different schema version and close of unplaced Content", () => {
		const layout = classicLayout();
		expect(
			reject(layout, {
				type: "undo",
				snapshot: { ...layout, schemaVersion: 99 },
			}).reason,
		).toBe("invalid-command");
		expect(
			reject(layout, { type: "close", contentId: contentIdFrom("ghost") })
				.reason,
		).toBe("unknown-content");
	});
});

describe("placement never carries authority", () => {
	it("layout state stores only references, ordering, and constraints", () => {
		const content: ContentRef = {
			id: contentIdFrom("app:instance-1:main"),
			kind: "app",
			extensions: { presentation: "main" },
		};
		const next = commit(classicLayout(), {
			type: "open",
			content,
			target: { role: "auxiliary" },
		});
		expect(containerByRole(next, "auxiliary").stack.at(-1)).toEqual(content);
	});
});
