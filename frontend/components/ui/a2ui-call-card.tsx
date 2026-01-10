"use client";

import { A2UIRenderer } from "@/components/a2ui/A2UIRenderer";
import { A2UISurfaceManager } from "@/lib/a2ui/state";
import type {
	A2UIMessage,
	A2UISurfaceState,
	A2UIUserAction,
} from "@/lib/a2ui/types";
import { setValueAtPath } from "@/lib/a2ui/types";
import { cn } from "@/lib/utils";
import {
	CheckCircle2,
	ChevronRight,
	Loader2,
	MessageSquare,
} from "lucide-react";
import { useCallback, useMemo, useRef, useState } from "react";

interface A2UICallCardProps {
	surfaceId: string;
	messages: A2UIMessage[];
	blocking?: boolean;
	requestId?: string;
	answered?: boolean;
	answeredAction?: string;
	answeredAt?: Date;
	onAction?: (action: A2UIUserAction) => void;
	defaultCollapsed?: boolean;
}

/**
 * Renders an A2UI surface in a card format similar to ToolCallCard.
 * Appears inline with messages, collapsible, shows answered state.
 */
export function A2UICallCard({
	surfaceId,
	messages,
	blocking,
	requestId,
	answered,
	answeredAction,
	answeredAt,
	onAction,
	defaultCollapsed = false,
}: A2UICallCardProps) {
	const [isOpen, setIsOpen] = useState(!defaultCollapsed && !answered);
	const [dataModelVersion, setDataModelVersion] = useState(0);

	// Track data model changes - use ref to avoid recreating surface on every change
	const dataModelRef = useRef<Record<string, unknown>>({});

	// Create and populate surface manager
	// biome-ignore lint/correctness/useExhaustiveDependencies: dataModelVersion triggers re-merge of tracked data
	const surface = useMemo(() => {
		const manager = new A2UISurfaceManager();
		manager.processMessages(messages);
		// Get the first ready surface
		let foundSurface = manager.getSurface(surfaceId);
		if (!foundSurface || !foundSurface.isReady) {
			const allSurfaces = manager.getAllSurfaces();
			for (const [, s] of allSurfaces) {
				if (s.isReady) {
					foundSurface = s;
					break;
				}
			}
		}
		// Merge our tracked data model into the surface
		if (foundSurface) {
			Object.assign(foundSurface.dataModel, dataModelRef.current);
		}
		return foundSurface;
	}, [surfaceId, messages, dataModelVersion]);

	// Handle data model changes from inputs
	const handleDataChange = useCallback((path: string, value: unknown) => {
		// Update our tracked data model
		const key = path.replace(/^\//, "").split("/")[0];
		dataModelRef.current[key] = value;

		// Also update nested paths if needed
		if (path.includes("/")) {
			setValueAtPath(dataModelRef.current, path, value);
		}

		// Trigger re-render so surface picks up new data
		setDataModelVersion((v) => v + 1);
	}, []);

	// Handle user actions - include the tracked data model in context
	const handleAction = useCallback(
		(action: A2UIUserAction) => {
			if (onAction && !answered) {
				// Merge our tracked data into the action context
				const enrichedContext = {
					...action.context,
					...dataModelRef.current,
				};
				onAction({
					...action,
					surfaceId,
					context: enrichedContext,
				});
			}
		},
		[onAction, surfaceId, answered],
	);

	if (!surface || !surface.isReady) {
		return null;
	}

	// Determine status styling
	const getStatusClasses = () => {
		if (answered) {
			return "border-border bg-card opacity-75";
		}
		if (blocking) {
			return "border-primary/30 bg-primary/5";
		}
		return "border-border bg-card";
	};

	// Get status icon
	const getStatusIcon = () => {
		if (answered) {
			return <CheckCircle2 className="w-3.5 h-3.5 text-primary" />;
		}
		if (blocking) {
			return <Loader2 className="w-3.5 h-3.5 text-primary animate-spin" />;
		}
		return <MessageSquare className="w-3.5 h-3.5 text-primary" />;
	};

	// Get title text
	const getTitle = () => {
		if (answered && answeredAction) {
			return `Answered: ${answeredAction}`;
		}
		if (blocking) {
			return "Waiting for input";
		}
		return "Interactive UI";
	};

	return (
		<div
			className={cn(
				"rounded-lg border transition-all duration-200",
				getStatusClasses(),
			)}
		>
			<button
				type="button"
				onClick={() => setIsOpen(!isOpen)}
				className="w-full flex items-center gap-2 px-3 py-2 text-left cursor-pointer hover:bg-muted/50"
			>
				<ChevronRight
					className={cn(
						"w-4 h-4 text-muted-foreground transition-transform duration-200 flex-shrink-0",
						isOpen && "rotate-90",
					)}
				/>

				{getStatusIcon()}

				<span className="flex-1 text-sm font-medium text-foreground truncate">
					{getTitle()}
				</span>

				{answeredAt && (
					<span className="text-xs text-foreground/60 dark:text-muted-foreground flex-shrink-0">
						{answeredAt.toLocaleTimeString([], {
							hour: "2-digit",
							minute: "2-digit",
						})}
					</span>
				)}
			</button>

			{isOpen && (
				<div className="px-3 pb-3 border-t border-border pt-2">
					<A2UIRenderer
						surface={surface}
						onAction={answered ? undefined : handleAction}
						onDataChange={handleDataChange}
					/>
				</div>
			)}
		</div>
	);
}

export default A2UICallCard;
