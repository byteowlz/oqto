/**
 * A2UI List Component
 *
 * Renders a list of children with optional scrolling.
 */

import type { A2UIComponentInstance, A2UISurfaceState } from "@/lib/a2ui/types";
import { cn } from "@/lib/utils";
import { ComponentRenderer } from "../A2UIRenderer";

interface A2UIListProps {
	props: Record<string, unknown>;
	dataModel: Record<string, unknown>;
	surface: A2UISurfaceState;
	onAction?: (actionName: string, context: Record<string, unknown>) => void;
	onDataChange?: (path: string, value: unknown) => void;
	resolveChildren: (childIds: string[]) => A2UIComponentInstance[];
}

export function A2UIList({
	props,
	surface,
	onAction,
	onDataChange,
	resolveChildren,
}: A2UIListProps) {
	const children = props.children as
		| { explicitList?: string[]; template?: unknown }
		| undefined;
	const direction =
		(props.direction as "vertical" | "horizontal") || "vertical";
	const alignment =
		(props.alignment as "start" | "center" | "end" | "stretch") || "stretch";

	// Get child IDs from explicit list
	const childIds = children?.explicitList || [];
	const childInstances = resolveChildren(childIds);

	const alignmentClass = {
		start: "items-start",
		center: "items-center",
		end: "items-end",
		stretch: "items-stretch",
	}[alignment];

	return (
		<div
			className={cn(
				"flex gap-2 overflow-auto",
				direction === "vertical" ? "flex-col" : "flex-row",
				alignmentClass,
			)}
		>
			{childInstances.map((child) => (
				<ComponentRenderer
					key={child.id}
					instance={child}
					surface={surface}
					onAction={
						onAction
							? (action) => onAction(action.name, action.context)
							: undefined
					}
					onDataChange={onDataChange}
				/>
			))}
		</div>
	);
}
