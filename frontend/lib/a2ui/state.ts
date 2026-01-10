/**
 * A2UI Surface State Manager
 *
 * Manages the state of A2UI surfaces including components, data model,
 * and rendering lifecycle.
 */

import type {
	A2UIComponentInstance,
	A2UIMessage,
	A2UIStyles,
	A2UISurfaceState,
	BeginRendering,
	DataModelUpdate,
	DeleteSurface,
	SurfaceUpdate,
} from "./types";
import { parseDataEntries, setValueAtPath } from "./types";

/**
 * Manages multiple A2UI surfaces
 */
export class A2UISurfaceManager {
	private surfaces: Map<string, A2UISurfaceState> = new Map();
	private listeners: Set<(surfaceId: string) => void> = new Set();

	/**
	 * Process an A2UI message and update the appropriate surface
	 */
	processMessage(message: A2UIMessage): string | null {
		if ("surfaceUpdate" in message) {
			return this.handleSurfaceUpdate(message.surfaceUpdate);
		}
		if ("dataModelUpdate" in message) {
			return this.handleDataModelUpdate(message.dataModelUpdate);
		}
		if ("beginRendering" in message) {
			return this.handleBeginRendering(message.beginRendering);
		}
		if ("deleteSurface" in message) {
			return this.handleDeleteSurface(message.deleteSurface);
		}
		return null;
	}

	/**
	 * Process multiple A2UI messages (e.g., from a JSONL stream)
	 */
	processMessages(messages: A2UIMessage[]): void {
		for (const message of messages) {
			this.processMessage(message);
		}
	}

	/**
	 * Get a surface by ID
	 */
	getSurface(surfaceId: string): A2UISurfaceState | undefined {
		return this.surfaces.get(surfaceId);
	}

	/**
	 * Get all surfaces
	 */
	getAllSurfaces(): Map<string, A2UISurfaceState> {
		return this.surfaces;
	}

	/**
	 * Check if a surface exists
	 */
	hasSurface(surfaceId: string): boolean {
		return this.surfaces.has(surfaceId);
	}

	/**
	 * Subscribe to surface updates
	 */
	subscribe(listener: (surfaceId: string) => void): () => void {
		this.listeners.add(listener);
		return () => this.listeners.delete(listener);
	}

	/**
	 * Clear all surfaces
	 */
	clear(): void {
		this.surfaces.clear();
		this.notifyAll();
	}

	// =========================================================================
	// Private Methods
	// =========================================================================

	private handleSurfaceUpdate(update: SurfaceUpdate): string {
		const { surfaceId, components } = update;

		// Get or create surface state
		let surface = this.surfaces.get(surfaceId);
		if (!surface) {
			surface = this.createSurface(surfaceId);
		}

		// Add/update components in the adjacency list
		for (const component of components) {
			surface.components.set(component.id, component);
		}

		this.notify(surfaceId);
		return surfaceId;
	}

	private handleDataModelUpdate(update: DataModelUpdate): string {
		const { surfaceId, path, contents } = update;

		// Get or create surface state
		let surface = this.surfaces.get(surfaceId);
		if (!surface) {
			surface = this.createSurface(surfaceId);
		}

		// Parse the data entries
		const data = parseDataEntries(contents);

		// Apply to data model at the specified path
		if (!path || path === "/") {
			// Replace entire data model
			Object.assign(surface.dataModel, data);
		} else {
			// Update at specific path
			for (const [key, value] of Object.entries(data)) {
				const fullPath = path.endsWith("/")
					? `${path}${key}`
					: `${path}/${key}`;
				setValueAtPath(surface.dataModel, fullPath, value);
			}
		}

		this.notify(surfaceId);
		return surfaceId;
	}

	private handleBeginRendering(rendering: BeginRendering): string {
		const { surfaceId, root, catalogId, styles } = rendering;

		// Get or create surface state
		let surface = this.surfaces.get(surfaceId);
		if (!surface) {
			surface = this.createSurface(surfaceId);
		}

		// Set rendering parameters
		surface.rootId = root;
		surface.catalogId = catalogId;
		surface.styles = styles;
		surface.isReady = true;

		this.notify(surfaceId);
		return surfaceId;
	}

	private handleDeleteSurface(deletion: DeleteSurface): string {
		const { surfaceId } = deletion;
		this.surfaces.delete(surfaceId);
		this.notify(surfaceId);
		return surfaceId;
	}

	private createSurface(surfaceId: string): A2UISurfaceState {
		const surface: A2UISurfaceState = {
			surfaceId,
			components: new Map(),
			dataModel: {},
			isReady: false,
		};
		this.surfaces.set(surfaceId, surface);
		return surface;
	}

	private notify(surfaceId: string): void {
		const listenersArray = Array.from(this.listeners);
		for (const listener of listenersArray) {
			listener(surfaceId);
		}
	}

	private notifyAll(): void {
		const surfaceIds = Array.from(this.surfaces.keys());
		for (const surfaceId of surfaceIds) {
			this.notify(surfaceId);
		}
	}
}

/**
 * Get a component from a surface by ID
 */
export function getComponent(
	surface: A2UISurfaceState,
	componentId: string,
): A2UIComponentInstance | undefined {
	return surface.components.get(componentId);
}

/**
 * Get all children of a container component
 */
export function getChildren(
	surface: A2UISurfaceState,
	componentId: string,
): A2UIComponentInstance[] {
	const component = surface.components.get(componentId);
	if (!component) return [];

	const props = Object.values(component.component)[0] as Record<
		string,
		unknown
	>;

	// Check for single child
	if ("child" in props && typeof props.child === "string") {
		const child = surface.components.get(props.child);
		return child ? [child] : [];
	}

	// Check for children (Row, Column, List)
	if ("children" in props) {
		const children = props.children as {
			explicitList?: string[];
			template?: { componentId: string; dataBinding: string };
		};

		if (children.explicitList) {
			return children.explicitList
				.map((id) => surface.components.get(id))
				.filter((c): c is A2UIComponentInstance => c !== undefined);
		}

		// Template-based children would need data model resolution
		// This is handled by the renderer
	}

	return [];
}

/**
 * Build a tree representation of the surface for debugging
 */
export function buildTreeString(
	surface: A2UISurfaceState,
	componentId: string = surface.rootId || "",
	indent = 0,
): string {
	const component = surface.components.get(componentId);
	if (!component) return `${" ".repeat(indent)}[missing: ${componentId}]\n`;

	const type = Object.keys(component.component)[0];
	let result = `${" ".repeat(indent)}${type} (${componentId})\n`;

	const children = getChildren(surface, componentId);
	for (const child of children) {
		result += buildTreeString(surface, child.id, indent + 2);
	}

	return result;
}
