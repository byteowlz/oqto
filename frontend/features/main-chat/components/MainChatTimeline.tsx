"use client";

import {
	type PiSessionFile,
	listMainChatPiSessions,
} from "@/features/main-chat/api";
import { formatSessionDate, generateReadableId } from "@/lib/session-utils";
import { cn } from "@/lib/utils";
import { useCallback, useEffect, useRef, useState } from "react";

export interface MainChatTimelineProps {
	/** Assistant name */
	assistantName: string;
	/** Currently active session ID (the one visible at top of viewport) */
	activeSessionId: string | null;
	/** Callback when a session dot is clicked */
	onSessionClick: (sessionId: string) => void;
	/** Callback when sessions are loaded */
	onSessionsLoaded?: (sessions: PiSessionFile[]) => void;
}

/**
 * Vertical timeline showing Main Chat sessions as connected dots.
 * The active session (visible at top of viewport) is highlighted.
 */
export function MainChatTimeline({
	assistantName,
	activeSessionId,
	onSessionClick,
	onSessionsLoaded,
}: MainChatTimelineProps) {
	const [sessions, setSessions] = useState<PiSessionFile[]>([]);
	const [loading, setLoading] = useState(true);

	// Load sessions
	useEffect(() => {
		if (!assistantName) return;

		setLoading(true);
		listMainChatPiSessions()
			.then((data) => {
				// Sort by started_at ascending (oldest first, so timeline goes top to bottom)
				const sorted = [...data].sort(
					(a, b) =>
						new Date(a.started_at).getTime() - new Date(b.started_at).getTime(),
				);
				setSessions(sorted);
				onSessionsLoaded?.(sorted);
			})
			.catch((err) => {
				console.error("Failed to load main chat sessions:", err);
				setSessions([]);
			})
			.finally(() => setLoading(false));
	}, [assistantName, onSessionsLoaded]);

	if (loading || sessions.length === 0) {
		return null;
	}

	return (
		<div className="flex flex-col items-center py-2 px-1">
			{/* Timeline line */}
			<div className="relative">
				{/* Vertical line */}
				<div className="absolute left-1/2 top-0 bottom-0 w-0.5 bg-border -translate-x-1/2" />

				{/* Session dots */}
				<div className="relative flex flex-col gap-1">
					{sessions.map((session, index) => {
						const isActive = session.id === activeSessionId;
						const isFirst = index === 0;
						const isLast = index === sessions.length - 1;

						return (
							<TimelineDot
								key={session.id}
								session={session}
								isActive={isActive}
								isFirst={isFirst}
								isLast={isLast}
								onClick={() => onSessionClick(session.id)}
							/>
						);
					})}
				</div>
			</div>
		</div>
	);
}

interface TimelineDotProps {
	session: PiSessionFile;
	isActive: boolean;
	isFirst: boolean;
	isLast: boolean;
	onClick: () => void;
}

function TimelineDot({
	session,
	isActive,
	isFirst,
	isLast,
	onClick,
}: TimelineDotProps) {
	const [showTooltip, setShowTooltip] = useState(false);

	const formattedDate = formatSessionDate(
		new Date(session.started_at).getTime(),
	);
	const title = session.title || formattedDate;

	return (
		<div className="relative group">
			<button
				type="button"
				onClick={onClick}
				onMouseEnter={() => setShowTooltip(true)}
				onMouseLeave={() => setShowTooltip(false)}
				className={cn(
					"relative z-10 w-2.5 h-2.5 transition-all",
					"border border-background",
					isActive
						? "bg-primary scale-125"
						: "bg-muted-foreground/40 hover:bg-muted-foreground/60 hover:scale-110",
				)}
				title={title}
			/>

			{/* Tooltip */}
			{showTooltip && (
				<div
					className={cn(
						"absolute left-full ml-2 top-1/2 -translate-y-1/2",
						"bg-popover text-popover-foreground text-xs px-2 py-1 rounded shadow-md",
						"whitespace-nowrap z-50 pointer-events-none",
					)}
				>
					<div className="font-medium">
						{session.title || "Untitled"}
						<span className="opacity-60">
							[{generateReadableId(session.id)}]
						</span>
					</div>
					<div className="text-muted-foreground">{formattedDate}</div>
					{session.message_count > 0 && (
						<div className="text-muted-foreground">
							{session.message_count} messages
						</div>
					)}
				</div>
			)}
		</div>
	);
}

/**
 * Hook to track which session is currently at the top of the viewport.
 * Returns the session ID that should be highlighted in the timeline.
 */
export function useActiveSessionTracker(
	sessions: PiSessionFile[],
	containerRef: React.RefObject<HTMLElement>,
): string | null {
	const [activeSessionId, setActiveSessionId] = useState<string | null>(null);

	useEffect(() => {
		const container = containerRef.current;
		if (!container || sessions.length === 0) return;

		// Create a map of session boundaries (cumulative message counts)
		// This would need to be populated based on actual message positions

		const handleScroll = () => {
			// For now, just use the first session as active
			// In a full implementation, we'd track scroll position and map to sessions
			if (sessions.length > 0 && !activeSessionId) {
				setActiveSessionId(sessions[sessions.length - 1].id);
			}
		};

		container.addEventListener("scroll", handleScroll);
		handleScroll(); // Initial check

		return () => container.removeEventListener("scroll", handleScroll);
	}, [sessions, containerRef, activeSessionId]);

	return activeSessionId;
}
