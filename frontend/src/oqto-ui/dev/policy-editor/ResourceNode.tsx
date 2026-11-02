import type { CSSProperties } from "react";
import { useTranslation } from "react-i18next";
import { AccessChips, OperationRow } from "./OperationRow";
import { ResourceMark } from "./ResourceMark";
import type { OperationView } from "./access";
import type { Box } from "./layout";
import type { GrantMode, Resource } from "./resources";

export type NodeEmphasis = "subject" | "linked" | "muted" | "normal";
export interface NodeView {
	resource: Resource;
	box: Box;
	editing: boolean;
	emphasis: NodeEmphasis;
	selected: boolean;
	expanded: boolean;
	note: string;
	operations: readonly OperationView[];
}
export interface NodeActions {
	onSelect: () => void;
	onEdit?: () => void;
	onPropose: (operation: string, mode: GrantMode) => void;
}
interface ResourceNodeProps {
	view: NodeView;
	actions: NodeActions;
}
export function ResourceNode({ view, actions: props }: ResourceNodeProps) {
	const { resource, box, editing, emphasis, expanded, operations } = view;
	const { t } = useTranslation();
	const changed = operations.some((operation) => operation.proposed);
	return (
		<div
			className="resource-node"
			data-kind={resource.kind}
			data-emphasis={emphasis}
			data-selected={view.selected}
			data-expanded={expanded}
			data-changed={changed}
			style={
				{
					"--node-x": `${box.x}px`,
					"--node-y": `${box.y}px`,
					"--node-w": `${box.w}px`,
					"--node-h": `${box.h}px`,
				} as CSSProperties
			}
		>
			<button
				type="button"
				className="resource-node-head"
				aria-pressed={view.selected}
				aria-expanded={operations.length && editing ? expanded : undefined}
				aria-label={t("{{name}}, {{kind}}", {
					name: resource.name,
					kind: t(resource.kind),
				})}
				onClick={props.onSelect}
			>
				<ResourceMark resource={resource} />
				<span className="resource-node-text">
					<strong translate="no">{resource.name}</strong>
					<small>{view.note}</small>
				</span>
				{!expanded &&
					!props.onEdit &&
					operations.length > 0 &&
					emphasis !== "muted" && <AccessChips operations={operations} />}
			</button>
			{props.onEdit && (
				<button
					type="button"
					className="resource-node-action"
					aria-label={t("Edit access for {{name}}", { name: resource.name })}
					onClick={props.onEdit}
				>
					{t("Edit access")}
				</button>
			)}
			{expanded && (
				<div className="resource-node-ops">
					{operations.map((operation) => (
						<OperationRow
							key={operation.id}
							operation={operation}
							resourceName={resource.name}
							editable={editing}
							onPropose={props.onPropose}
						/>
					))}
				</div>
			)}
		</div>
	);
}
