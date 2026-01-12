/**
 * A2UI Button Component
 *
 * Clean button with proper context resolution from data model.
 */

import { Button } from "@/components/ui/button";
import type {
	A2UIComponentInstance,
	A2UISurfaceState,
	ButtonComponent,
} from "@/lib/a2ui/types";
import { resolveBoundValue } from "@/lib/a2ui/types";
import { ComponentRenderer } from "../A2UIRenderer";

interface A2UIButtonProps {
	props: Record<string, unknown>;
	dataModel: Record<string, unknown>;
	surface: A2UISurfaceState;
	onAction: (actionName: string, context: Record<string, unknown>) => void;
	onDataChange?: (path: string, value: unknown) => void;
	resolveChildren: (childIds: string[]) => A2UIComponentInstance[];
}

export function A2UIButton({
	props,
	dataModel,
	surface,
	onAction,
	onDataChange,
}: A2UIButtonProps) {
	const buttonProps = props as unknown as ButtonComponent;
	const childId = buttonProps.child;
	const action = buttonProps.action;
	const isPrimary = buttonProps.primary;

	const handleClick = () => {
		if (action) {
			// Resolve action context values from the surface's data model
			const context: Record<string, unknown> = {};
			if (action.context) {
				for (const item of action.context) {
					// If the context item has a key but no value, look it up in the data model
					if (item.key) {
						if (item.value) {
							context[item.key] = resolveBoundValue(
								item.value,
								dataModel,
								null,
							);
						} else {
							// Look up by key directly in data model
							context[item.key] =
								dataModel[item.key] ?? surface.dataModel[item.key] ?? null;
						}
					}
				}
			}
			onAction(action.name, context);
		}
	};

	// Get the child component to render inside the button
	const childComponent = surface.components.get(childId);

	return (
		<Button
			variant={isPrimary ? "default" : "outline"}
			onClick={handleClick}
			size="default"
		>
			{childComponent ? (
				<ComponentRenderer
					instance={childComponent}
					surface={surface}
					onAction={(a) => onAction(a.name, a.context)}
					onDataChange={onDataChange}
				/>
			) : (
				childId
			)}
		</Button>
	);
}
