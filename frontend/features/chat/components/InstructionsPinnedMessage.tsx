import { ChevronDown, ChevronRight, Pin } from "lucide-react";
import { useCallback, useState } from "react";

import { MarkdownRenderer } from "@/components/data-display";
import { cn } from "@/lib/utils";

interface InstructionsPinnedMessageProps {
	content: string;
	workspacePath?: string | null;
	className?: string;
}

function buildStorageKey(
	workspacePath: string | null | undefined,
): string | null {
	if (!workspacePath) return null;
	const safe = workspacePath.replace(/[^a-zA-Z0-9._-]+/g, "_");
	return `oqto:instructions:collapsed:${safe}`;
}

function readInitialCollapsed(key: string | null): boolean {
	if (!key || typeof window === "undefined") return false;
	try {
		return window.localStorage.getItem(key) === "1";
	} catch {
		return false;
	}
}

/**
 * Renders the contents of the workspace's INSTRUCTIONS.md as a pinned,
 * assistant-styled card at the top of a chat. Not a real message: not sent
 * to the agent, not persisted to history. Sourced directly from disk by
 * `useWorkspaceInstructions`. Click the header to collapse/expand; the state
 * is persisted per workspace in localStorage.
 */
export function InstructionsPinnedMessage({
	content,
	workspacePath,
	className,
}: InstructionsPinnedMessageProps) {
	const storageKey = buildStorageKey(workspacePath);
	const [collapsed, setCollapsed] = useState<boolean>(() =>
		readInitialCollapsed(storageKey),
	);

	const toggle = useCallback(() => {
		setCollapsed((prev) => {
			const next = !prev;
			if (storageKey) {
				try {
					if (next) {
						window.localStorage.setItem(storageKey, "1");
					} else {
						window.localStorage.removeItem(storageKey);
					}
				} catch {
					/* localStorage unavailable; ignore */
				}
			}
			return next;
		});
	}, [storageKey]);

	return (
		<div
			className={cn(
				"group transition-colors duration-200 overflow-hidden min-w-0 max-w-full",
				"sm:mr-8 bg-muted/50 border border-border border-dashed",
				className,
			)}
			data-instructions-pinned="true"
		>
			<button
				type="button"
				onClick={toggle}
				aria-expanded={!collapsed}
				aria-controls="instructions-pinned-body"
				className={cn(
					"flex items-center gap-1 sm:gap-2 px-2 sm:px-3 py-1.5 sm:py-2 overflow-hidden w-full text-left hover:bg-muted/70 transition-colors",
					!collapsed && "border-b border-dashed border-border",
				)}
			>
				{collapsed ? (
					<ChevronRight className="w-3 h-3 sm:w-4 sm:h-4 text-muted-foreground flex-shrink-0" />
				) : (
					<ChevronDown className="w-3 h-3 sm:w-4 sm:h-4 text-muted-foreground flex-shrink-0" />
				)}
				<Pin className="w-3 h-3 sm:w-4 sm:h-4 text-primary flex-shrink-0" />
				<span className="text-sm font-medium text-foreground truncate min-w-0">
					INSTRUCTIONS.md
				</span>
				<span className="text-[10px] sm:text-[11px] text-muted-foreground truncate">
					pinned from workspace
				</span>
			</button>
			{!collapsed && (
				<div
					id="instructions-pinned-body"
					className="px-2 sm:px-3 py-2 sm:py-3"
				>
					<MarkdownRenderer content={content} />
				</div>
			)}
		</div>
	);
}
