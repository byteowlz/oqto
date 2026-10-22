import { type Resource, operationsFor, resources } from "./resources";

export type Direction = "horizontal" | "vertical";
/** Columns (left→right) or rows (top→down) of the resource canvas. */
export type Axis =
	| "account"
	| "knowledge"
	| "workspace"
	| "workdir"
	| "service";
export interface Box {
	x: number;
	y: number;
	w: number;
	h: number;
}

const MAIN: Record<Axis, number> = {
	account: 0,
	knowledge: 148,
	workspace: 372,
	workdir: 580,
	service: 848,
};
const KIND_WIDTH: Record<Resource["kind"], number> = {
	account: 120,
	workspace: 176,
	workdir: 200,
	wiki: 196,
	memory: 196,
	service: 216,
};
const AXIS_OF: Record<Resource["kind"], Axis> = {
	account: "account",
	workspace: "workspace",
	workdir: "workdir",
	wiki: "knowledge",
	memory: "knowledge",
	service: "service",
};
const HEAD = 40;
const ROW = 28;
const GAP = 6;
export const COLUMN_X = MAIN;
export const HEAD_HEIGHT = HEAD;
export const AXIS_LABELS = [
	"ACCOUNT",
	"KNOWLEDGE",
	"WORKSPACES",
	"WORK DIRECTORIES",
	"SERVICES",
] as const;

function nodeHeight(resource: Resource, expanded: boolean) {
	if (!expanded) return HEAD;
	const rows = operationsFor(resource.kind).length * ROW;
	return HEAD + rows + (resource.kind === "workdir" ? 34 : 6);
}

/** Display geometry only. Collapsed nodes keep their axis compact. */
export function resourceLayout(
	expanded: ReadonlySet<string>,
	direction: Direction,
) {
	const axes: Axis[] = [
		"account",
		"knowledge",
		"workspace",
		"workdir",
		"service",
	];
	const boxes = new Map<string, Box>();
	if (direction === "vertical") {
		let y = 0;
		const positions: number[] = [];
		for (const axis of axes) {
			positions.push(y);
			const items = resources.filter((item) => AXIS_OF[item.kind] === axis);
			const columns = Math.min(4, items.length);
			for (let start = 0; start < items.length; start += columns) {
				const row = items.slice(start, start + columns);
				const rowHeight = Math.max(
					...row.map((item) => nodeHeight(item, expanded.has(item.id))),
				);
				row.forEach((item, index) => {
					const w = KIND_WIDTH[item.kind];
					boxes.set(item.id, {
						x: (960 - columns * 240) / 2 + index * 240,
						y,
						w,
						h: nodeHeight(item, expanded.has(item.id)),
					});
				});
				y += rowHeight + GAP;
			}
			y += 52;
		}
		return { boxes, width: 960, height: y - 52, positions };
	}
	const built = axes.map((axis) => {
		const items = resources.filter((item) => AXIS_OF[item.kind] === axis);
		const placements = items.map((item) => {
			const w = KIND_WIDTH[item.kind];
			const h = nodeHeight(item, expanded.has(item.id));
			return {
				id: item.id,
				box:
					direction === "horizontal"
						? { x: MAIN[axis], y: 0, w, h }
						: { x: 0, y: MAIN[axis], w, h },
			};
		});
		const stacked = placements.reduce(
			(sum, placement) =>
				sum +
				(direction === "horizontal" ? placement.box.h : placement.box.w) +
				GAP,
			-GAP,
		);
		return { axis, placements, extent: Math.max(0, stacked) };
	});
	const extent = Math.max(0, ...built.map((column) => column.extent));
	for (const column of built) {
		const offset = (extent - column.extent) / 2;
		let cursor = offset;
		for (const placement of column.placements) {
			const box = placement.box;
			if (direction === "horizontal") box.y = cursor;
			else box.x = cursor;
			cursor += (direction === "horizontal" ? box.h : box.w) + GAP;
			boxes.set(placement.id, box);
		}
	}
	// Ownership bars follow actual workdir bounds, including expanded operations.
	for (const workspace of resources.filter(
		(item) => item.kind === "workspace",
	)) {
		const children = resources
			.filter((item) => item.kind === "workdir" && item.parent === workspace.id)
			.flatMap((item) => boxes.get(item.id) ?? []);
		const bar = boxes.get(workspace.id);
		if (!bar || !children.length) continue;
		bar.y = Math.min(...children.map((child) => child.y));
		bar.h = Math.max(...children.map((child) => child.y + child.h)) - bar.y;
	}
	const workdirs = resources
		.filter((item) => item.kind === "workdir")
		.flatMap((item) => boxes.get(item.id) ?? []);
	const account = boxes.get("account");
	if (account && workdirs.length) {
		account.y = Math.min(...workdirs.map((box) => box.y));
		account.h = Math.max(...workdirs.map((box) => box.y + box.h)) - account.y;
	}
	const serviceAxis = built.find((column) => column.axis === "service");
	const serviceDepth = Math.max(
		HEAD,
		...(serviceAxis?.placements.map((placement) => placement.box.h) ?? [HEAD]),
	);
	return {
		boxes,
		positions: Object.values(MAIN),
		width:
			direction === "horizontal" ? MAIN.service + KIND_WIDTH.service : extent,
		height: direction === "horizontal" ? extent : MAIN.service + serviceDepth,
	};
}
