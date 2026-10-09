/**
 * The work directory's issues, loaded once on mount and refreshed after a
 * status change. Loading is driven by interaction (mount, reload, a status
 * move), never by rendering.
 */

import { useMountEffect } from "@/hooks/use-mount-effect";
import { useCallback, useRef, useState } from "react";
import type {
	Issue,
	IssueHost,
	IssueStatus,
} from "../platform/issues-contract";

export interface IssueBoard {
	readonly issues: readonly Issue[];
	readonly loading: boolean;
	readonly failed: boolean;
	reload(): void;
	move(issueId: string, status: IssueStatus): void;
}

interface BoardState {
	readonly issues: readonly Issue[];
	readonly loading: boolean;
	readonly failed: boolean;
}

const START: BoardState = { issues: [], loading: true, failed: false };

export function useIssues(host: IssueHost, workspacePath: string): IssueBoard {
	const [state, setState] = useState<BoardState>(START);
	const live = useRef(true);

	const reload = useCallback(() => {
		setState((current) => ({ ...current, loading: true, failed: false }));
		host.list(workspacePath).then(
			(issues) => {
				if (live.current) setState({ issues, loading: false, failed: false });
			},
			() => {
				if (live.current)
					setState({ issues: [], loading: false, failed: true });
			},
		);
	}, [host, workspacePath]);

	useMountEffect(() => {
		live.current = true;
		reload();
		return () => {
			live.current = false;
		};
	});

	const move = useCallback(
		(issueId: string, status: IssueStatus) => {
			// Optimistic: the row moves group immediately, the reload confirms it.
			setState((current) => ({
				...current,
				issues: current.issues.map((issue) =>
					issue.id === issueId ? { ...issue, status } : issue,
				),
			}));
			host.setStatus(workspacePath, issueId, status).then(reload, reload);
		},
		[host, workspacePath, reload],
	);

	return { ...state, reload, move };
}
