export type VisualRuntimeMode =
	| "offline_strict"
	| "offline_prefer"
	| "online_flexible";

export type VisualRuntimeCapability =
	| "morphdom"
	| "mermaid"
	| "layout_elk"
	| "chartjs"
	| "three";

export interface VisualRuntimeDiagnostic {
	level: "info" | "warn" | "error";
	message: string;
}

export interface PrepareVisualRuntimeInput {
	html: string;
	mode: VisualRuntimeMode;
}

export interface PrepareVisualRuntimeResult {
	html: string;
	requires: VisualRuntimeCapability[];
	diagnostics: VisualRuntimeDiagnostic[];
}
