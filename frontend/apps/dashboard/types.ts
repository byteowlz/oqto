import type { CodexBarUsagePayload } from "@/lib/control-plane-client";

export type TrxIssue = {
	id: string;
	title: string;
	status: string;
	priority: number;
	issue_type: string;
	updated_at: string;
};

export type FeedItem = {
	id: string;
	title: string;
	link?: string;
	date?: string;
};

export type FeedState = {
	title: string;
	items: FeedItem[];
	loading: boolean;
	error?: string;
};

export type CodexBarState = {
	available: boolean;
	loading: boolean;
	error?: string;
	payload: CodexBarUsagePayload[];
};

export type DashboardCardSpan = 3 | 6 | 9 | 12;

export type DashboardLayoutCard = {
	visible: boolean;
	span: DashboardCardSpan;
};

export type DashboardLayoutConfig = {
	version: 1;
	order: string[];
	cards: Record<string, DashboardLayoutCard>;
	feeds?: string[];
};

export type DashboardRegistryCard = {
	id: string;
	title: string;
	description?: string;
	kind: "markdown" | "query";
	config?: {
		content?: string;
		url?: string;
		method?: string;
		headers?: Record<string, string>;
	};
};

export type DashboardRegistryConfig = {
	version: 1;
	cards: DashboardRegistryCard[];
};

export type BuiltinCardDefinition = {
	id: string;
	title: string;
	description?: string;
	defaultSpan: DashboardCardSpan;
};
