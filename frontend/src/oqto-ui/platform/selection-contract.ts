/** Public, versioned selection data. Presentation and permissions are separate. */
export interface Choice {
	id: string;
	label: string;
	description?: string;
	recommended?: boolean;
}

export interface TextConstraint {
	label?: string;
	maxLength?: number;
	multiline?: boolean;
}

export interface Question {
	id: string;
	title: string;
	header?: string;
	description?: string;
	groupId?: string;
	kind: "single" | "multiple" | "text";
	options?: Choice[];
	required?: boolean;
	allowOther?: boolean;
	minSelections?: number;
	maxSelections?: number;
	maxLength?: number;
	multiline?: boolean;
	note?: TextConstraint;
}

export interface Group {
	id: string;
	title: string;
}

export interface ReviewItem {
	id: string;
	label: string;
	description?: string;
	groupId?: string;
	required?: boolean;
}

export interface QuestionsSpec {
	version: 1;
	mode: "questions";
	title: string;
	description?: string;
	questions: Question[];
	groups?: Group[];
}

export interface ReviewSpec {
	version: 1;
	mode: "review";
	title: string;
	description?: string;
	items: ReviewItem[];
	choices: Choice[];
	multiple?: boolean;
	allowOther?: boolean;
	note?: TextConstraint;
	groups?: Group[];
}

export type SelectionSpec = QuestionsSpec | ReviewSpec;

/** Renderers consume a normalized document, not different host-specific schemas. */
export interface NormalizedSpec {
	version: 1;
	mode: "questions" | "review";
	title: string;
	description?: string;
	questions: Question[];
	groups: Group[];
}

export interface Answer {
	answered: boolean;
	/** Explicit deferral survives host switches; absent means ordinary unanswered. */
	disposition?: "skipped" | "unsure";
	selectedIds: string[];
	/** Standalone text answer or an explicitly enabled Other answer. */
	text?: string;
	/** Supplemental text never silently changes the decision. */
	note?: string;
}

export type Answers = Record<string, Answer>;

export interface SelectionRecord {
	version: 1;
	id: string;
	scopeId: string;
	revision: number;
	state: "draft" | "submitted" | "cancelled";
	spec: NormalizedSpec;
	answers: Answers;
	createdAt: string;
	updatedAt: string;
}

export type TuiOutcome =
	| { action: "submit"; answers: Answers }
	| { action: "cancel"; answers: Answers }
	| { action: "browser"; answers: Answers };
