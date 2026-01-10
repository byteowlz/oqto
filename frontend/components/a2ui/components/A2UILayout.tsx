/**
 * A2UI Layout Components (Row, Column)
 */

import type {
	A2UIComponentInstance,
	A2UISurfaceState,
	ColumnComponent,
	RowComponent,
} from "@/lib/a2ui/types";
import { cn } from "@/lib/utils";
import { ComponentRenderer } from "../A2UIRenderer";

interface LayoutProps {
	props: Record<string, unknown>;
	dataModel: Record<string, unknown>;
	surface: A2UISurfaceState;
	onAction: (actionName: string, context: Record<string, unknown>) => void;
	onDataChange?: (path: string, value: unknown) => void;
	resolveChildren: (childIds: string[]) => A2UIComponentInstance[];
}

const distributionClasses: Record<string, string> = {
	start: "justify-start",
	center: "justify-center",
	end: "justify-end",
	spaceBetween: "justify-between",
	spaceAround: "justify-around",
	spaceEvenly: "justify-evenly",
};

const alignmentClasses: Record<string, string> = {
	start: "items-start",
	center: "items-center",
	end: "items-end",
	stretch: "items-stretch",
};

export function A2UIRow({
	props,
	dataModel,
	surface,
	onAction,
	onDataChange,
	resolveChildren,
}: LayoutProps) {
	const rowProps = props as unknown as RowComponent;
	const children = rowProps.children;

	// Get child IDs
	const childIds = children.explicitList || [];
	const childComponents = resolveChildren(childIds);

	const className = cn(
		"flex flex-row gap-2",
		distributionClasses[rowProps.distribution || "start"],
		alignmentClasses[rowProps.alignment || "center"],
	);

	return (
		<div className={className}>
			{childComponents.map((child) => (
				<div
					key={child.id}
					style={child.weight ? { flex: child.weight } : undefined}
				>
					<ComponentRenderer
						instance={child}
						surface={surface}
						onAction={(a) => onAction(a.name, a.context)}
						onDataChange={onDataChange}
					/>
				</div>
			))}
		</div>
	);
}

export function A2UIColumn({
	props,
	dataModel,
	surface,
	onAction,
	onDataChange,
	resolveChildren,
}: LayoutProps) {
	const columnProps = props as unknown as ColumnComponent;
	const children = columnProps.children;

	// Get child IDs
	const childIds = children.explicitList || [];
	const childComponents = resolveChildren(childIds);

	const className = cn(
		"flex flex-col gap-2",
		distributionClasses[columnProps.distribution || "start"],
		alignmentClasses[columnProps.alignment || "stretch"],
	);

	return (
		<div className={className}>
			{childComponents.map((child) => (
				<div
					key={child.id}
					style={child.weight ? { flex: child.weight } : undefined}
				>
					<ComponentRenderer
						instance={child}
						surface={surface}
						onAction={(a) => onAction(a.name, a.context)}
						onDataChange={onDataChange}
					/>
				</div>
			))}
		</div>
	);
}
