import type { Box } from "./layout";
import type { Direction } from "./layout";
import { HEAD_HEIGHT } from "./layout";
import { type Draft, byId, connections, resources } from "./resources";

export interface Edge {
	from: string;
	to: string;
	kind: "tree" | "subject" | "proposed";
}
/** Membership edges plus the focused subject's configured reach. */
export function resourceEdges(
	subject: string,
	drafts: readonly Draft[],
): Edge[] {
	const edges: Edge[] = [];
	for (const item of resources) {
		if (item.kind === "workspace")
			edges.push({ from: "account", to: item.id, kind: "tree" });
		else if (
			item.kind === "wiki" ||
			item.kind === "memory" ||
			item.kind === "workdir"
		) {
			if (item.parent)
				edges.push({ from: item.parent, to: item.id, kind: "tree" });
		}
	}
	const linked = new Set(connections[subject] ?? []);
	for (const target of linked)
		if (target !== subject)
			edges.push({ from: subject, to: target, kind: "subject" });
	for (const target of reachedByDrafts(subject, drafts))
		if (!linked.has(target) && target !== subject)
			edges.push({ from: subject, to: target, kind: "proposed" });
	return edges;
}
export function reachedByDrafts(subject: string, drafts: readonly Draft[]) {
	return new Set(
		drafts
			.filter(
				(draft) =>
					draft.subject === subject &&
					draft.mode !== "deny" &&
					draft.resource !== subject,
			)
			.map((draft) => draft.resource),
	);
}
export function edgePath(from: Box, to: Box, direction: Direction) {
	if (direction === "horizontal") {
		const x1 = from.x + from.w;
		const y1 = from.y + from.h / 2;
		const y2 = to.y + Math.min(to.h, HEAD_HEIGHT) / 2;
		const bend = Math.max(28, (to.x - x1) / 2);
		return `M${x1} ${y1} C${x1 + bend} ${y1}, ${to.x - bend} ${y2}, ${to.x} ${y2}`;
	}
	const x1 = from.x + from.w / 2;
	const y1 = from.y + from.h;
	const x2 = to.x + to.w / 2;
	const y2 = to.y;
	const bend = Math.max(28, (y2 - y1) / 2);
	return `M${x1} ${y1} C${x1} ${y1 + bend}, ${x2} ${y2 - bend}, ${x2} ${y2}`;
}
export const isKnowledge = (id: string) => {
	const kind = byId(id)?.kind;
	return kind === "wiki" || kind === "memory";
};
