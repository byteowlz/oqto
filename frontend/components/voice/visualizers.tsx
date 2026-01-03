/**
 * Pluggable visualizer system for voice mode.
 *
 * Allows easy addition of custom visualizers by implementing the
 * VisualizerComponent interface and registering them here.
 */

import type { VisualizerType, VoiceState } from "@/lib/voice/types";
import type { ComponentType } from "react";
import { KittVisualizer } from "./KittVisualizer";
import { OrbVisualizer } from "./OrbVisualizer";

/**
 * Props that all visualizers must accept.
 */
export interface VisualizerProps {
	/** Current voice state */
	state: VoiceState;
	/** VAD progress (0-1) */
	vadProgress: number;
	/** Input volume (0-1) */
	inputVolume: number;
	/** Output volume (0-1) */
	outputVolume: number;
	/** Whether audio is enabled */
	audioEnabled: boolean;
	/** Optional class name */
	className?: string;
}

/**
 * Visualizer metadata for UI display.
 */
export interface VisualizerMeta {
	/** Unique identifier */
	id: VisualizerType;
	/** Display name */
	name: string;
	/** Description */
	description: string;
	/** Preview thumbnail (optional) */
	thumbnail?: string;
}

/**
 * Registry of available visualizers.
 */
export const VISUALIZER_REGISTRY: Record<
	string,
	{
		component: ComponentType<VisualizerProps>;
		meta: VisualizerMeta;
	}
> = {
	orb: {
		component: OrbVisualizer as ComponentType<VisualizerProps>,
		meta: {
			id: "orb",
			name: "Orb",
			description: "Fluid 3D orb with particle effects",
		},
	},
	kitt: {
		component: KittVisualizer as ComponentType<VisualizerProps>,
		meta: {
			id: "kitt",
			name: "K.I.T.T.",
			description: "Classic Knight Rider LED bar",
		},
	},
};

/**
 * Get a visualizer component by ID.
 */
export function getVisualizer(
	id: VisualizerType,
): ComponentType<VisualizerProps> | null {
	return VISUALIZER_REGISTRY[id]?.component ?? null;
}

/**
 * Get all available visualizer metadata.
 */
export function getAvailableVisualizers(): VisualizerMeta[] {
	return Object.values(VISUALIZER_REGISTRY).map((v) => v.meta);
}

/**
 * Check if a visualizer ID is valid.
 */
export function isValidVisualizer(id: string): id is VisualizerType {
	return id in VISUALIZER_REGISTRY;
}

/**
 * Dynamic visualizer component that renders the selected visualizer.
 */
export function DynamicVisualizer({
	type,
	...props
}: VisualizerProps & { type: VisualizerType }) {
	const Visualizer = getVisualizer(type);

	if (!Visualizer) {
		// Fallback to orb if unknown type
		const FallbackVisualizer = VISUALIZER_REGISTRY.orb.component;
		return <FallbackVisualizer {...props} />;
	}

	return <Visualizer {...props} />;
}
