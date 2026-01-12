/**
 * A2UI Surface Renderer
 *
 * Main component that renders an A2UI surface from its state.
 * Uses the adjacency list model where components reference each other by ID.
 */

import type {
	A2UIComponent,
	A2UIComponentInstance,
	A2UISurfaceState,
	A2UIUserAction,
} from "@/lib/a2ui/types";
import {
	getComponentProps,
	getComponentType,
	resolveBoundValue,
} from "@/lib/a2ui/types";
import { useCallback, useMemo } from "react";
import { A2UIAudioPlayer } from "./components/A2UIAudioPlayer";
import { A2UIButton } from "./components/A2UIButton";
import { A2UICard } from "./components/A2UICard";
import { A2UICheckBox } from "./components/A2UICheckBox";
import { A2UIDateTimeInput } from "./components/A2UIDateTimeInput";
import { A2UIDivider } from "./components/A2UIDivider";
import { A2UIIcon } from "./components/A2UIIcon";
import { A2UIImage } from "./components/A2UIImage";
import { A2UIColumn, A2UIRow } from "./components/A2UILayout";
import { A2UIList } from "./components/A2UIList";
import { A2UIModal } from "./components/A2UIModal";
import { A2UIMultipleChoice } from "./components/A2UIMultipleChoice";
import { A2UISlider } from "./components/A2UISlider";
import { A2UITabs } from "./components/A2UITabs";
import { A2UIText } from "./components/A2UIText";
import { A2UITextField } from "./components/A2UITextField";
import { A2UIVideo } from "./components/A2UIVideo";

export interface A2UIRendererProps {
	/** The surface state to render */
	surface: A2UISurfaceState;
	/** Callback when user performs an action */
	onAction?: (action: A2UIUserAction) => void;
	/** Callback when data model changes (for two-way binding) */
	onDataChange?: (path: string, value: unknown) => void;
	/** Additional CSS classes */
	className?: string;
}

export interface ComponentRendererProps {
	instance: A2UIComponentInstance;
	surface: A2UISurfaceState;
	onAction?: (action: A2UIUserAction) => void;
	onDataChange?: (path: string, value: unknown) => void;
}

/**
 * Renders an A2UI surface
 */
export function A2UIRenderer({
	surface,
	onAction,
	onDataChange,
	className,
}: A2UIRendererProps) {
	if (!surface.isReady || !surface.rootId) {
		return null;
	}

	const rootComponent = surface.components.get(surface.rootId);
	if (!rootComponent) {
		return (
			<div className="text-destructive text-sm">
				Missing root component: {surface.rootId}
			</div>
		);
	}

	return (
		<div className={className}>
			<ComponentRenderer
				instance={rootComponent}
				surface={surface}
				onAction={onAction}
				onDataChange={onDataChange}
			/>
		</div>
	);
}

/**
 * Renders a single A2UI component instance
 */
export function ComponentRenderer({
	instance,
	surface,
	onAction,
	onDataChange,
}: ComponentRendererProps) {
	const componentType = getComponentType(instance.component);
	const props = getComponentProps(instance.component);

	// Create action handler that includes component context
	const handleAction = useCallback(
		(actionName: string, context: Record<string, unknown>) => {
			if (onAction) {
				onAction({
					name: actionName,
					surfaceId: surface.surfaceId,
					sourceComponentId: instance.id,
					timestamp: new Date().toISOString(),
					context,
				});
			}
		},
		[onAction, surface.surfaceId, instance.id],
	);

	// Resolve children for container components
	const resolveChildren = useCallback(
		(childIds: string[]) => {
			return childIds
				.map((id) => surface.components.get(id))
				.filter((c): c is A2UIComponentInstance => c !== undefined);
		},
		[surface.components],
	);

	// Common props for all component renderers
	const commonProps = {
		surface,
		onAction: handleAction,
		onDataChange,
		resolveChildren,
	};

	switch (componentType) {
		case "Text":
			return <A2UIText props={props} dataModel={surface.dataModel} />;

		case "Button":
			return (
				<A2UIButton
					props={props}
					dataModel={surface.dataModel}
					{...commonProps}
				/>
			);

		case "Row":
			return (
				<A2UIRow props={props} dataModel={surface.dataModel} {...commonProps} />
			);

		case "Column":
			return (
				<A2UIColumn
					props={props}
					dataModel={surface.dataModel}
					{...commonProps}
				/>
			);

		case "Card":
			return (
				<A2UICard
					props={props}
					dataModel={surface.dataModel}
					{...commonProps}
				/>
			);

		case "TextField":
			return (
				<A2UITextField
					props={props}
					dataModel={surface.dataModel}
					{...commonProps}
				/>
			);

		case "CheckBox":
			return (
				<A2UICheckBox
					props={props}
					dataModel={surface.dataModel}
					{...commonProps}
				/>
			);

		case "MultipleChoice":
			return (
				<A2UIMultipleChoice
					props={props}
					dataModel={surface.dataModel}
					{...commonProps}
				/>
			);

		case "Image":
			return <A2UIImage props={props} dataModel={surface.dataModel} />;

		case "Icon":
			return <A2UIIcon props={props} dataModel={surface.dataModel} />;

		case "Divider":
			return <A2UIDivider props={props} />;

		case "Slider":
			return (
				<A2UISlider
					props={props}
					dataModel={surface.dataModel}
					{...commonProps}
				/>
			);

		case "Tabs":
			return (
				<A2UITabs
					props={props}
					dataModel={surface.dataModel}
					{...commonProps}
				/>
			);

		case "Video":
			return <A2UIVideo props={props} dataModel={surface.dataModel} />;

		case "AudioPlayer":
			return <A2UIAudioPlayer props={props} dataModel={surface.dataModel} />;

		case "List":
			return (
				<A2UIList
					props={props}
					dataModel={surface.dataModel}
					{...commonProps}
				/>
			);

		case "Modal":
			return (
				<A2UIModal
					props={props}
					dataModel={surface.dataModel}
					{...commonProps}
				/>
			);

		case "DateTimeInput":
			return (
				<A2UIDateTimeInput
					props={props}
					dataModel={surface.dataModel}
					onDataChange={onDataChange}
				/>
			);

		default:
			return (
				<div className="text-muted-foreground text-xs p-2 border border-dashed rounded">
					Unknown component: {componentType}
				</div>
			);
	}
}
