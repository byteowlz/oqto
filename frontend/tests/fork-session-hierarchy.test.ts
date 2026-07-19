import type { ChatSession } from "@/lib/control-plane-client";
import { flattenForkDescendants } from "@/src/routes/app-shell/SidebarSessions";
import { describe, expect, it } from "vitest";

function session(id: string, parentId: string | null): ChatSession {
	return {
		id,
		parent_id: parentId,
		title: id,
		readable_id: "",
		workspace_path: "/tmp/work",
		project_name: "work",
		created_at: 0,
		updated_at: 0,
		version: "0",
		is_child: parentId !== null,
		model: null,
		provider: null,
	};
}

describe("fork Session hierarchy", () => {
	it("renders recursive fork descendants in stable depth-first order", () => {
		const children = new Map<string, ChatSession[]>([
			["parent", [session("child-a", "parent"), session("child-b", "parent")]],
			["child-a", [session("grandchild", "child-a")]],
		]);

		expect(
			flattenForkDescendants(children, "parent").map(({ session, depth }) => [
				session.id,
				depth,
			]),
		).toEqual([
			["child-a", 1],
			["grandchild", 2],
			["child-b", 1],
		]);
	});

	it("fails safe on malformed cycles instead of recursing forever", () => {
		const children = new Map<string, ChatSession[]>([
			["parent", [session("child", "parent")]],
			["child", [session("parent", "child")]],
		]);
		expect(
			flattenForkDescendants(children, "parent").map(
				({ session }) => session.id,
			),
		).toEqual(["child"]);
	});
});
