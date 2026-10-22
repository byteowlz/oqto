import type { PolicyNode } from "./model";

/** Display geometry only; never computes permission or inheritance semantics. */
export function boundaryLayout(
	nodes: readonly PolicyNode[],
	expanded: ReadonlySet<string>,
) {
	const positions = new Map<string, { x: number; y: number; depth: number }>();
	const visible: PolicyNode[] = [];
	let leaf = 0;
	const visit = (
		node: PolicyNode,
		depth: number,
		ancestors: Set<string>,
	): number => {
		if (ancestors.has(node.id)) return 56;
		visible.push(node);
		const children = expanded.has(node.id)
			? nodes.filter((child) => child.parentId === node.id)
			: [];
		const ys = children.map((child) =>
			visit(child, depth + 1, new Set([...ancestors, node.id])),
		);
		const y = ys.length ? (ys[0] + ys[ys.length - 1]) / 2 : 64 + leaf++ * 72;
		positions.set(node.id, { x: 48 + depth * 248, y, depth });
		return y;
	};
	for (const root of nodes.filter((node) => node.parentId === null))
		visit(root, 0, new Set());
	return {
		positions,
		visible,
		width: Math.max(800, ...[...positions.values()].map((p) => p.x + 244)),
		height: Math.max(540, leaf * 72 + 96),
	};
}
