/**
 * A2UI Modal Component
 *
 * Renders a modal dialog with an entry point (trigger) and content.
 */

import type { A2UIComponentInstance, A2UISurfaceState } from "@/lib/a2ui/types";
import { cn } from "@/lib/utils";
import { X } from "lucide-react";
import { useState } from "react";
import { ComponentRenderer } from "../A2UIRenderer";

interface A2UIModalProps {
	props: Record<string, unknown>;
	dataModel: Record<string, unknown>;
	surface: A2UISurfaceState;
	onAction?: (actionName: string, context: Record<string, unknown>) => void;
	onDataChange?: (path: string, value: unknown) => void;
	resolveChildren: (childIds: string[]) => A2UIComponentInstance[];
}

export function A2UIModal({
	props,
	surface,
	onAction,
	onDataChange,
}: A2UIModalProps) {
	const [isOpen, setIsOpen] = useState(false);

	const entryPointChildId = props.entryPointChild as string | undefined;
	const contentChildId = props.contentChild as string | undefined;

	const entryPoint = entryPointChildId
		? surface.components.get(entryPointChildId)
		: undefined;
	const content = contentChildId
		? surface.components.get(contentChildId)
		: undefined;

	return (
		<>
			{/* Entry point (trigger) */}
			<button
				type="button"
				onClick={() => setIsOpen(true)}
				className="cursor-pointer text-left"
			>
				{entryPoint ? (
					<ComponentRenderer
						instance={entryPoint}
						surface={surface}
						onAction={
							onAction
								? (action) => onAction(action.name, action.context)
								: undefined
						}
						onDataChange={onDataChange}
					/>
				) : (
					<span className="text-primary underline">Open Modal</span>
				)}
			</button>

			{/* Modal overlay and content */}
			{isOpen && (
				<dialog
					open
					className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 m-0 p-0 w-full h-full max-w-none max-h-none border-none"
					onClick={() => setIsOpen(false)}
					onKeyDown={(e) => {
						if (e.key === "Escape") {
							setIsOpen(false);
						}
					}}
				>
					<div
						className="relative bg-background border border-border rounded-lg shadow-lg max-w-lg w-full mx-4 p-4"
						onClick={(e) => e.stopPropagation()}
						onKeyDown={() => {}}
					>
						{/* Close button */}
						<button
							type="button"
							onClick={() => setIsOpen(false)}
							className="absolute top-2 right-2 p-1 text-muted-foreground hover:text-foreground"
							aria-label="Close modal"
						>
							<X className="w-4 h-4" />
						</button>

						{/* Modal content */}
						<div className="pt-4">
							{content ? (
								<ComponentRenderer
									instance={content}
									surface={surface}
									onAction={
										onAction
											? (action) => onAction(action.name, action.context)
											: undefined
									}
									onDataChange={onDataChange}
								/>
							) : (
								<div className="text-muted-foreground text-sm">
									No modal content
								</div>
							)}
						</div>
					</div>
				</dialog>
			)}
		</>
	);
}
