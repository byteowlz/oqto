import type { ComponentType } from "react";
import type { OqtoCapabilityKey } from "./host";

export interface OqtoAppIconProps {
	className?: string;
}

export interface OqtoAppDefinition {
	/** Stable id, used for routing and persisted selection. */
	id: string;
	/** Display name. */
	title: string;
	/** Optional one-line description. */
	description?: string;
	/** Optional icon component (e.g. a lucide icon). */
	icon?: ComponentType<OqtoAppIconProps>;
	/**
	 * The app root. Mounted inside the host provider tree; it reads the host via
	 * useOqtoHost() rather than taking it as a prop, so the same component works
	 * standalone and embedded.
	 */
	component: ComponentType;
	/** Capabilities the app intends to use (advisory; for future gating). */
	requestedCapabilities?: OqtoCapabilityKey[];
}

/** A registered oqto mini-app. */
export type OqtoApp = OqtoAppDefinition;
