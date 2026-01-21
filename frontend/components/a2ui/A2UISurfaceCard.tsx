/**
 * A2UI Surface Card
 *
 * Renders an A2UI surface in a card format suitable for chat timelines.
 * Handles both blocking (waiting for user input) and non-blocking surfaces.
 */

import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader } from "@/components/ui/card";
import { A2UISurfaceManager } from "@/lib/a2ui/state";
import type { A2UIMessage, A2UIUserAction } from "@/lib/a2ui/types";
import { cn } from "@/lib/utils";
import { Loader2, X } from "lucide-react";
import { useCallback, useMemo } from "react";
import { A2UIRenderer } from "./A2UIRenderer";

export interface A2UISurfaceCardProps {
	/** Unique surface ID */
	surfaceId: string;
	/** A2UI messages defining the surface */
	messages: A2UIMessage[];
	/** Whether the agent is waiting for user response */
	blocking?: boolean;
	/** Request ID for blocking surfaces */
	requestId?: string;
	/** Callback when user performs an action */
	onAction?: (action: A2UIUserAction) => void;
	/** Callback to dismiss/close the surface */
	onDismiss?: () => void;
	/** Additional CSS classes */
	className?: string;
	/** Whether the surface is loading */
	isLoading?: boolean;
	/** Whether the user has already answered */
	answered?: boolean;
	/** The action name that was selected */
	answeredAction?: string;
	/** When the user answered */
	answeredAt?: Date;
}

/**
 * Renders an A2UI surface in a card format
 */
export function A2UISurfaceCard({
	surfaceId,
	messages,
	blocking,
	requestId,
	onAction,
	onDismiss,
	className,
	isLoading,
	answered,
	answeredAction,
	answeredAt,
}: A2UISurfaceCardProps) {
	// Create and populate surface manager
	const surface = useMemo(() => {
		const manager = new A2UISurfaceManager();
		manager.processMessages(messages);
		// Try to get the surface by ID, or get the first ready surface
		// (the surfaceId prop may differ from the internal surface ID in messages)
		let foundSurface = manager.getSurface(surfaceId);
		if (!foundSurface || !foundSurface.isReady) {
			// Fallback: get any ready surface from the messages
			const allSurfaces = manager.getAllSurfaces();
			for (const [, s] of allSurfaces) {
				if (s.isReady) {
					foundSurface = s;
					break;
				}
			}
		}
		return foundSurface;
	}, [surfaceId, messages]);

	// Handle user actions
	const handleAction = useCallback(
		(action: A2UIUserAction) => {
			if (onAction) {
				onAction({
					...action,
					surfaceId,
				});
			}
		},
		[onAction, surfaceId],
	);

	// Handle data model changes (for two-way binding)
	const handleDataChange = useCallback((path: string, value: unknown) => {
		// Avoid logging on every keystroke; enable with localStorage.setItem("debug:a2ui", "1").
		try {
			if (import.meta.env.DEV && localStorage.getItem("debug:a2ui") === "1") {
				console.debug("[A2UI] Data change:", path, value);
			}
		} catch {
			// ignore
		}
	}, []);

	if (!surface || !surface.isReady) {
		if (isLoading) {
			return (
				<Card className={cn("border-primary/20", className)}>
					<CardContent className="p-4 flex items-center justify-center gap-2">
						<Loader2 className="w-4 h-4 animate-spin" />
						<span className="text-sm text-muted-foreground">Loading...</span>
					</CardContent>
				</Card>
			);
		}
		return null;
	}

	return (
		<Card
			className={cn(
				"overflow-hidden",
				blocking && !answered && "border-primary/50 shadow-md",
				answered && "opacity-75 border-muted",
				className,
			)}
		>
			{/* Header for blocking surfaces or answered state */}
			{(blocking || answered) && (
				<CardHeader className="p-2 border-b bg-primary/5 flex flex-row items-center justify-between">
					<span className="text-xs font-medium text-primary">
						{answered
							? `Answered: ${answeredAction || "action"}${answeredAt ? ` at ${answeredAt.toLocaleTimeString()}` : ""}`
							: "Waiting for your input"}
					</span>
					{onDismiss && !answered && (
						<Button
							variant="ghost"
							size="icon"
							className="h-6 w-6"
							onClick={onDismiss}
						>
							<X className="h-3 w-3" />
						</Button>
					)}
				</CardHeader>
			)}

			{/* Surface content */}
			<CardContent className={cn("p-4", (blocking || answered) && "pt-3")}>
				<A2UIRenderer
					surface={surface}
					onAction={answered ? undefined : handleAction}
					onDataChange={handleDataChange}
				/>
			</CardContent>
		</Card>
	);
}

/**
 * Inline A2UI surface for embedding in message parts
 */
export function A2UISurfaceInline({
	surfaceId,
	messages,
	onAction,
	className,
}: Omit<A2UISurfaceCardProps, "blocking" | "requestId" | "onDismiss">) {
	// Create and populate surface manager
	const surface = useMemo(() => {
		const manager = new A2UISurfaceManager();
		manager.processMessages(messages);
		// Try to get the surface by ID, or get the first ready surface
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
		return foundSurface;
	}, [surfaceId, messages]);

	// Handle user actions
	const handleAction = useCallback(
		(action: A2UIUserAction) => {
			if (onAction) {
				onAction({
					...action,
					surfaceId,
				});
			}
		},
		[onAction, surfaceId],
	);

	if (!surface || !surface.isReady) {
		return null;
	}

	return (
		<div className={cn("py-2", className)}>
			<A2UIRenderer
				surface={surface}
				onAction={handleAction}
				onDataChange={() => {}}
			/>
		</div>
	);
}
