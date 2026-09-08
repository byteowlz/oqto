/**
 * The issue-tracker port OqtoUI's Issues pane talks to. One work directory
 * at a time, the same way the Files port is scoped: the pane never learns
 * where issues are stored or how they travel.
 */

export type IssueStatus =
	| "open"
	| "ready"
	| "in_progress"
	| "blocked"
	| "closed";

export interface Issue {
	readonly id: string;
	readonly title: string;
	readonly status: IssueStatus;
	/** 0 is the most urgent, matching the tracker's own scale. */
	readonly priority: number;
	readonly kind: string;
	readonly parentId: string | null;
	readonly blockedBy: readonly string[];
	readonly labels: readonly string[];
}

export interface IssueHost {
	list(workspacePath: string): Promise<readonly Issue[]>;
	/** Moves an issue to a status; closing takes an optional reason. */
	setStatus(
		workspacePath: string,
		issueId: string,
		status: IssueStatus,
		reason?: string,
	): Promise<void>;
}
