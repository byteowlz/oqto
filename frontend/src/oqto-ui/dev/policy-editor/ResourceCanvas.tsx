import type { CSSProperties } from "react";
import { useTranslation } from "react-i18next";
import { type NodeEmphasis, ResourceNode } from "./ResourceNode";
import { operationViews } from "./access";
import { edgePath, resourceEdges } from "./edges";
import { AXIS_LABELS, type Direction, resourceLayout } from "./layout";
import {
	type Draft,
	type GrantMode,
	type Resource,
	connections,
	resources,
} from "./resources";
import { useCanvasSize } from "./useCanvasSize";

const LABEL_SPACE = 34;
export interface CanvasState {
	editing: boolean;
	subject: string;
	selected: string;
	expanded: ReadonlySet<string>;
	drafts: readonly Draft[];
	direction: Direction;
}
export interface CanvasActions {
	onSelect: (id: string) => void;
	onEdit: () => void;
	onPropose: (resource: string, operation: string, mode: GrantMode) => void;
}
interface ResourceCanvasProps {
	state: CanvasState;
	actions: CanvasActions;
}
const KNOWLEDGE = new Set(["wiki", "memory"]);
const editableKind = (subject: string, item: Resource) =>
	item.id === subject || KNOWLEDGE.has(item.kind) || item.kind === "service";

export function ResourceCanvas({ state, actions }: ResourceCanvasProps) {
	const { editing, subject, drafts, direction } = state;
	const { t } = useTranslation();
	const canvas = useCanvasSize();
	const layout = resourceLayout(state.expanded, direction);
	const horizontal = direction === "horizontal";
	const scale = Math.min(
		1.3,
		horizontal
			? (canvas.size.inlineSize - 48) / layout.width
			: (canvas.size.inlineSize - 48 - 130) / layout.width,
		horizontal
			? (canvas.size.blockSize - 24) / (layout.height + LABEL_SPACE)
			: (canvas.size.blockSize - 24) / layout.height,
	);
	const linked = new Set(connections[subject] ?? []);
	const emphasis = (item: Resource): NodeEmphasis => {
		if (item.id === subject) return "subject";
		if (KNOWLEDGE.has(item.kind) || item.kind === "service")
			return linked.has(item.id) ? "linked" : "normal";
		return editing && item.kind !== "account" ? "muted" : "normal";
	};
	const note = (item: Resource) => {
		if (item.kind === "workspace")
			return t("{{access}} · {{count}} workdirs", {
				access: t(item.accessKey),
				count: resources.filter((child) => child.parent === item.id).length,
			});
		if (KNOWLEDGE.has(item.kind) || item.kind === "service") {
			const count = Object.values(connections).filter((list) =>
				list.includes(item.id),
			).length;
			return t("{{kind}} · {{count}} connections", {
				kind: t(item.noteKey),
				count,
			});
		}
		if (item.id === subject)
			return editing ? t("Editing access") : t("Focused workdir");
		return t(item.noteKey);
	};
	const positions = AXIS_LABELS.map((_, index) => layout.positions[index]);
	return (
		<div className="resource-canvas" ref={canvas.ref}>
			<div
				className="resource-world"
				data-direction={direction}
				style={
					{
						"--world-w": `${horizontal ? layout.width : layout.width + 130}px`,
						"--world-h": `${horizontal ? layout.height + LABEL_SPACE : layout.height}px`,
						"--world-scale": scale,
					} as CSSProperties
				}
			>
				<div className="resource-columns">
					{AXIS_LABELS.map((label, index) => (
						<span
							key={label}
							style={{ "--axis-pos": `${positions[index]}px` } as CSSProperties}
						>
							{t(label)}
						</span>
					))}
				</div>
				<div className="resource-plane">
					<svg
						className="resource-edges"
						aria-hidden="true"
						width={layout.width}
						height={layout.height}
					>
						{resourceEdges(subject, drafts).map((edge) => {
							const from = layout.boxes.get(edge.from);
							const to = layout.boxes.get(edge.to);
							return from && to ? (
								<path
									key={`${edge.kind}:${edge.from}:${edge.to}`}
									data-kind={edge.kind}
									d={edgePath(from, to, direction)}
								/>
							) : null;
						})}
					</svg>
					{resources.map((item) => {
						const box = layout.boxes.get(item.id);
						if (!box) return null;
						const editable = editableKind(subject, item);
						return (
							<ResourceNode
								key={item.id}
								view={{
									resource: item,
									box,
									editing,
									emphasis: emphasis(item),
									selected: state.selected === item.id,
									expanded: state.expanded.has(item.id),
									note: note(item),
									operations: editable
										? operationViews(subject, item, drafts)
										: [],
								}}
								actions={{
									onSelect: () => actions.onSelect(item.id),
									onEdit:
										!editing && item.id === subject
											? actions.onEdit
											: undefined,
									onPropose: (operation, mode) =>
										actions.onPropose(item.id, operation, mode),
								}}
							/>
						);
					})}
				</div>
			</div>
		</div>
	);
}
