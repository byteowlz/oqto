/**
 * A2UI v0.8 Type Definitions
 *
 * Based on the official A2UI specification from Google:
 * https://github.com/google/A2UI/tree/main/specification/0.8
 *
 * A2UI (Agent to UI) is a protocol for agents to generate rich, interactive
 * user interfaces that render natively across platforms.
 */

// =============================================================================
// Core Message Types
// =============================================================================

/**
 * A2UI message - exactly one of these must be present
 */
export type A2UIMessage =
	| { beginRendering: BeginRendering }
	| { surfaceUpdate: SurfaceUpdate }
	| { dataModelUpdate: DataModelUpdate }
	| { deleteSurface: DeleteSurface };

/**
 * Signals the client to begin rendering a surface
 */
export interface BeginRendering {
	/** Unique identifier for the UI surface */
	surfaceId: string;
	/** ID of the root component to render */
	root: string;
	/** Component catalog to use (defaults to standard catalog) */
	catalogId?: string;
	/** Styling information for the UI */
	styles?: A2UIStyles;
}

/**
 * Updates a surface with components
 */
export interface SurfaceUpdate {
	/** Unique identifier for the UI surface */
	surfaceId: string;
	/** List of components for the surface */
	components: A2UIComponentInstance[];
}

/**
 * Updates the data model for a surface
 */
export interface DataModelUpdate {
	/** Unique identifier for the UI surface */
	surfaceId: string;
	/** Path within the data model (e.g., '/user/name') */
	path?: string;
	/** Data entries to update */
	contents: DataEntry[];
}

/**
 * Signals the client to delete a surface
 */
export interface DeleteSurface {
	/** Unique identifier for the UI surface to delete */
	surfaceId: string;
}

// =============================================================================
// Component Instance
// =============================================================================

/**
 * A component instance in the adjacency list
 */
export interface A2UIComponentInstance {
	/** Unique identifier for this component */
	id: string;
	/** Component definition (wrapper with single key = component type) */
	component: A2UIComponent;
	/** Relative weight for flex layout (only when child of Row/Column) */
	weight?: number;
}

/**
 * Component wrapper - exactly one key which is the component type name
 */
export type A2UIComponent =
	| { Text: TextComponent }
	| { Image: ImageComponent }
	| { Icon: IconComponent }
	| { Video: VideoComponent }
	| { AudioPlayer: AudioPlayerComponent }
	| { Row: RowComponent }
	| { Column: ColumnComponent }
	| { List: ListComponent }
	| { Card: CardComponent }
	| { Tabs: TabsComponent }
	| { Divider: DividerComponent }
	| { Modal: ModalComponent }
	| { Button: ButtonComponent }
	| { CheckBox: CheckBoxComponent }
	| { TextField: TextFieldComponent }
	| { DateTimeInput: DateTimeInputComponent }
	| { MultipleChoice: MultipleChoiceComponent }
	| { Slider: SliderComponent };

// =============================================================================
// Bound Values (Data Binding)
// =============================================================================

/**
 * A value that can be literal or bound to the data model
 */
export interface BoundString {
	literalString?: string;
	path?: string;
}

export interface BoundNumber {
	literalNumber?: number;
	path?: string;
}

export interface BoundBoolean {
	literalBoolean?: boolean;
	path?: string;
}

export interface BoundArray {
	literalArray?: string[];
	path?: string;
}

export interface BoundValue {
	literalString?: string;
	literalNumber?: number;
	literalBoolean?: boolean;
	path?: string;
}

// =============================================================================
// Standard Catalog Components
// =============================================================================

export interface TextComponent {
	text: BoundString;
	usageHint?: "h1" | "h2" | "h3" | "h4" | "h5" | "caption" | "body";
}

export interface ImageComponent {
	url: BoundString;
	fit?: "contain" | "cover" | "fill" | "none" | "scale-down";
	usageHint?:
		| "icon"
		| "avatar"
		| "smallFeature"
		| "mediumFeature"
		| "largeFeature"
		| "header";
}

export type IconName =
	| "accountCircle"
	| "add"
	| "arrowBack"
	| "arrowForward"
	| "attachFile"
	| "calendarToday"
	| "call"
	| "camera"
	| "check"
	| "close"
	| "delete"
	| "download"
	| "edit"
	| "event"
	| "error"
	| "favorite"
	| "favoriteOff"
	| "folder"
	| "help"
	| "home"
	| "info"
	| "locationOn"
	| "lock"
	| "lockOpen"
	| "mail"
	| "menu"
	| "moreVert"
	| "moreHoriz"
	| "notificationsOff"
	| "notifications"
	| "payment"
	| "person"
	| "phone"
	| "photo"
	| "print"
	| "refresh"
	| "search"
	| "send"
	| "settings"
	| "share"
	| "shoppingCart"
	| "star"
	| "starHalf"
	| "starOff"
	| "upload"
	| "visibility"
	| "visibilityOff"
	| "warning";

export interface IconComponent {
	name: {
		literalString?: IconName;
		path?: string;
	};
}

export interface VideoComponent {
	url: BoundString;
}

export interface AudioPlayerComponent {
	url: BoundString;
	description?: BoundString;
}

/**
 * Children definition - either explicit list or template
 */
export interface Children {
	explicitList?: string[];
	template?: {
		componentId: string;
		dataBinding: string;
	};
}

export interface RowComponent {
	children: Children;
	distribution?:
		| "center"
		| "end"
		| "spaceAround"
		| "spaceBetween"
		| "spaceEvenly"
		| "start";
	alignment?: "start" | "center" | "end" | "stretch";
}

export interface ColumnComponent {
	children: Children;
	distribution?:
		| "start"
		| "center"
		| "end"
		| "spaceBetween"
		| "spaceAround"
		| "spaceEvenly";
	alignment?: "center" | "end" | "start" | "stretch";
}

export interface ListComponent {
	children: Children;
	direction?: "vertical" | "horizontal";
	alignment?: "start" | "center" | "end" | "stretch";
}

export interface CardComponent {
	child: string;
}

export interface TabsComponent {
	tabItems: Array<{
		title: BoundString;
		child: string;
	}>;
}

export interface DividerComponent {
	axis?: "horizontal" | "vertical";
}

export interface ModalComponent {
	entryPointChild: string;
	contentChild: string;
}

/**
 * Action to dispatch on user interaction
 */
export interface A2UIAction {
	name: string;
	context?: Array<{
		key: string;
		value: BoundValue;
	}>;
}

export interface ButtonComponent {
	child: string;
	action: A2UIAction;
	primary?: boolean;
}

export interface CheckBoxComponent {
	label: BoundString;
	value: BoundBoolean;
}

export interface TextFieldComponent {
	label: BoundString;
	text?: BoundString;
	textFieldType?: "date" | "longText" | "number" | "shortText" | "obscured";
	validationRegexp?: string;
}

export interface DateTimeInputComponent {
	value: BoundString;
	enableDate?: boolean;
	enableTime?: boolean;
}

export interface MultipleChoiceComponent {
	selections: BoundArray;
	options: Array<{
		label: BoundString;
		value: string;
	}>;
	maxAllowedSelections?: number;
}

export interface SliderComponent {
	value: BoundNumber;
	minValue?: number;
	maxValue?: number;
}

// =============================================================================
// Data Model
// =============================================================================

export interface DataEntry {
	key: string;
	valueString?: string;
	valueNumber?: number;
	valueBoolean?: boolean;
	valueMap?: DataEntry[];
}

// =============================================================================
// Styles
// =============================================================================

export interface A2UIStyles {
	font?: string;
	primaryColor?: string;
}

// =============================================================================
// User Action (Client -> Server)
// =============================================================================

/**
 * User action event sent from client to server
 */
export interface A2UIUserAction {
	name: string;
	surfaceId: string;
	sourceComponentId: string;
	timestamp: string;
	context: Record<string, unknown>;
}

// =============================================================================
// A2UI Surface State
// =============================================================================

/**
 * State of a single A2UI surface
 */
export interface A2UISurfaceState {
	surfaceId: string;
	catalogId?: string;
	rootId?: string;
	components: Map<string, A2UIComponentInstance>;
	dataModel: Record<string, unknown>;
	styles?: A2UIStyles;
	isReady: boolean;
}

// =============================================================================
// A2UI Message Part (for Octo chat integration)
// =============================================================================

/**
 * A2UI part embedded in a chat message
 */
export interface A2UIPart {
	type: "a2ui";
	/** Unique surface identifier */
	surface_id: string;
	/** A2UI messages (surfaceUpdate, dataModelUpdate, beginRendering) */
	messages: A2UIMessage[];
	/** If true, agent is waiting for user response */
	blocking?: boolean;
	/** Request ID for blocking requests */
	request_id?: string;
}

// =============================================================================
// Helper Functions
// =============================================================================

/**
 * Get the component type name from a component wrapper
 */
export function getComponentType(component: A2UIComponent): string {
	return Object.keys(component)[0];
}

/**
 * Get the component properties from a component wrapper
 */
export function getComponentProps(
	component: A2UIComponent,
): Record<string, unknown> {
	const type = getComponentType(component);
	return (component as Record<string, unknown>)[type] as Record<
		string,
		unknown
	>;
}

/**
 * Resolve a bound value against a data model
 */
export function resolveBoundValue<T>(
	bound:
		| {
				literalString?: string;
				literalNumber?: number;
				literalBoolean?: boolean;
				literalArray?: string[];
				path?: string;
		  }
		| undefined,
	dataModel: Record<string, unknown>,
	defaultValue: T,
): T {
	if (!bound) return defaultValue;

	// Check for literal values first
	if ("literalString" in bound && bound.literalString !== undefined) {
		return bound.literalString as T;
	}
	if ("literalNumber" in bound && bound.literalNumber !== undefined) {
		return bound.literalNumber as T;
	}
	if ("literalBoolean" in bound && bound.literalBoolean !== undefined) {
		return bound.literalBoolean as T;
	}
	if ("literalArray" in bound && bound.literalArray !== undefined) {
		return bound.literalArray as T;
	}

	// Resolve from data model path
	if (bound.path) {
		const value = getValueAtPath(dataModel, bound.path);
		if (value !== undefined) {
			return value as T;
		}
	}

	return defaultValue;
}

/**
 * Get a value from a nested object using a JSON pointer path
 */
export function getValueAtPath(
	obj: Record<string, unknown>,
	path: string,
): unknown {
	// Remove leading slash and split
	const parts = path.replace(/^\//, "").split("/");
	let current: unknown = obj;

	for (const part of parts) {
		if (current === null || current === undefined) {
			return undefined;
		}
		if (typeof current === "object") {
			current = (current as Record<string, unknown>)[part];
		} else {
			return undefined;
		}
	}

	return current;
}

/**
 * Set a value in a nested object using a JSON pointer path
 */
export function setValueAtPath(
	obj: Record<string, unknown>,
	path: string,
	value: unknown,
): void {
	const parts = path.replace(/^\//, "").split("/");
	let current = obj;

	for (let i = 0; i < parts.length - 1; i++) {
		const part = parts[i];
		if (!(part in current)) {
			current[part] = {};
		}
		current = current[part] as Record<string, unknown>;
	}

	current[parts[parts.length - 1]] = value;
}

/**
 * Parse data entries into a data model object
 */
export function parseDataEntries(
	entries: DataEntry[],
): Record<string, unknown> {
	const result: Record<string, unknown> = {};

	for (const entry of entries) {
		if (entry.valueString !== undefined) {
			result[entry.key] = entry.valueString;
		} else if (entry.valueNumber !== undefined) {
			result[entry.key] = entry.valueNumber;
		} else if (entry.valueBoolean !== undefined) {
			result[entry.key] = entry.valueBoolean;
		} else if (entry.valueMap !== undefined) {
			result[entry.key] = parseDataEntries(entry.valueMap);
		}
	}

	return result;
}
