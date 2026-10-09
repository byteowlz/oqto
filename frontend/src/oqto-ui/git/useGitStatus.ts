/**
 * The work directory's git state: what changed, and what a chosen path's
 * diff looks like. Loaded on mount and refreshed after every change to the
 * index, never during rendering.
 */

import { useMountEffect } from "@/hooks/use-mount-effect";
import { useCallback, useRef, useState } from "react";
import type {
	GitBranch,
	GitDiff,
	GitHost,
	GitStatus,
	RemoteOutcome,
} from "../platform/git-contract";

export interface GitView {
	readonly status: GitStatus | null;
	readonly loading: boolean;
	readonly failed: string | null;
	/** The diff on screen, or null when nothing is selected. */
	readonly diff: GitDiff | null;
	readonly branches: readonly GitBranch[];
	/** What the last remote operation reported, if anything. */
	readonly notice: string | null;
	reload(): void;
	switchTo(branch: string): void;
	runRemote(operation: RemoteOutcome["operation"]): void;
	show(path: string, staged: boolean): void;
	stage(paths: readonly string[], staged: boolean): void;
	commit(message: string): void;
}

interface GitState {
	readonly status: GitStatus | null;
	readonly loading: boolean;
	readonly failed: string | null;
	readonly diff: GitDiff | null;
	readonly branches: readonly GitBranch[];
	readonly notice: string | null;
}

const START: GitState = {
	status: null,
	loading: true,
	failed: null,
	diff: null,
	branches: [],
	notice: null,
};

export function useGitStatus(host: GitHost, workspacePath: string): GitView {
	const [state, setState] = useState<GitState>(START);
	const live = useRef(true);

	const reload = useCallback(() => {
		setState((current) => ({ ...current, loading: true, failed: null }));
		host.branches.list(workspacePath).then(
			(branches) => {
				if (live.current) setState((current) => ({ ...current, branches }));
			},
			() => {},
		);
		host.status(workspacePath).then(
			(status) => {
				if (live.current)
					setState((current) => ({ ...current, status, loading: false }));
			},
			(error: Error) => {
				if (live.current)
					setState((current) => ({
						...current,
						loading: false,
						failed: error.message,
					}));
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

	const show = useCallback(
		(path: string, staged: boolean) => {
			host.diff(workspacePath, path, staged).then(
				(diff) => {
					if (live.current) setState((current) => ({ ...current, diff }));
				},
				(error: Error) => {
					if (live.current)
						setState((current) => ({ ...current, failed: error.message }));
				},
			);
		},
		[host, workspacePath],
	);

	const stage = useCallback(
		(paths: readonly string[], staged: boolean) => {
			host.stage(workspacePath, paths, staged).then(reload, (error: Error) => {
				if (live.current)
					setState((current) => ({ ...current, failed: error.message }));
			});
		},
		[host, workspacePath, reload],
	);

	const commit = useCallback(
		(message: string) => {
			host.commit(workspacePath, message).then(
				() => {
					if (live.current) setState((current) => ({ ...current, diff: null }));
					reload();
				},
				(error: Error) => {
					if (live.current)
						setState((current) => ({ ...current, failed: error.message }));
				},
			);
		},
		[host, workspacePath, reload],
	);

	const switchTo = useCallback(
		(branch: string) => {
			host.branches
				.switch(workspacePath, branch)
				.then(reload, (error: Error) => {
					if (live.current)
						setState((current) => ({ ...current, failed: error.message }));
				});
		},
		[host, workspacePath, reload],
	);

	const runRemote = useCallback(
		(operation: RemoteOutcome["operation"]) => {
			setState((current) => ({ ...current, notice: null, failed: null }));
			host.remote(workspacePath, operation).then(
				(outcome) => {
					if (live.current)
						setState((current) => ({
							...current,
							notice: outcome.summary || outcome.operation,
						}));
					reload();
				},
				(error: Error) => {
					if (live.current)
						setState((current) => ({ ...current, failed: error.message }));
				},
			);
		},
		[host, workspacePath, reload],
	);

	return { ...state, reload, switchTo, runRemote, show, stage, commit };
}
