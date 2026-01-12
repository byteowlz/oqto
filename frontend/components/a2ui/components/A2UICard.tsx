/**
 * A2UI Card Component
 *
 * Clean card container with subtle styling.
 */

import type {
	A2UIComponentInstance,
	A2UISurfaceState,
	CardComponent,
} from "@/lib/a2ui/types";
import { ComponentRenderer } from "../A2UIRenderer";

interface A2UICardProps {
	props: Record<string, unknown>;
	dataModel: Record<string, unknown>;
	surface: A2UISurfaceState;
	onAction: (actionName: string, context: Record<string, unknown>) => void;
	onDataChange?: (path: string, value: unknown) => void;
	resolveChildren: (childIds: string[]) => A2UIComponentInstance[];
}

export function A2UICard({
	props,
	surface,
	onAction,
	onDataChange,
}: A2UICardProps) {
	const cardProps = props as unknown as CardComponent;
	const childId = cardProps.child;
	const childComponent = surface.components.get(childId);

	return (
		<div className="rounded-lg border border-border bg-card p-4 shadow-sm">
			{childComponent ? (
				<ComponentRenderer
					instance={childComponent}
					surface={surface}
					onAction={(a) => onAction(a.name, a.context)}
					onDataChange={onDataChange}
				/>
			) : (
				<span className="text-muted-foreground text-sm">Empty card</span>
			)}
		</div>
	);
}
