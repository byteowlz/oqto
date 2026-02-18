/**
 * Security prompt dialog component.
 *
 * Displays approval requests from oqto-guard and oqto-ssh-proxy,
 * allowing users to allow or deny access to protected resources.
 */

import {
	type Prompt,
	type PromptAction,
	getPromptIcon,
	getPromptTitle,
	getRemainingTime,
	usePrompts,
} from "@/hooks/use-prompts";
import { cn } from "@/lib/utils";
import { AnimatePresence, motion } from "framer-motion";
import {
	AlertTriangle,
	Check,
	Clock,
	FileKey,
	Key,
	Shield,
	X,
} from "lucide-react";
import { useEffect, useState } from "react";
import { Button } from "../ui/button";
import {
	Dialog,
	DialogContent,
	DialogDescription,
	DialogFooter,
	DialogHeader,
	DialogTitle,
} from "../ui/dialog";

// ============================================================================
// Single Prompt Card (for toast-style notifications)
// ============================================================================

interface PromptCardProps {
	prompt: Prompt;
	onRespond: (action: PromptAction) => void;
}

function PromptCard({ prompt, onRespond }: PromptCardProps) {
	const [remaining, setRemaining] = useState(getRemainingTime(prompt));

	// Update countdown
	useEffect(() => {
		const interval = setInterval(() => {
			setRemaining(getRemainingTime(prompt));
		}, 1000);
		return () => clearInterval(interval);
	}, [prompt]);

	const icon = getPromptIcon(prompt);
	const title = getPromptTitle(prompt);

	return (
		<motion.div
			initial={{ opacity: 0, y: -20, scale: 0.95 }}
			animate={{ opacity: 1, y: 0, scale: 1 }}
			exit={{ opacity: 0, y: -20, scale: 0.95 }}
			className={cn(
				"bg-card border rounded-lg shadow-lg p-4 w-[380px]",
				"border-amber-500/50",
			)}
		>
			{/* Header */}
			<div className="flex items-start gap-3 mb-3">
				<div className="flex-shrink-0 w-10 h-10 rounded-full bg-amber-500/10 flex items-center justify-center">
					<Shield className="w-5 h-5 text-amber-500" />
				</div>
				<div className="flex-1 min-w-0">
					<h4 className="font-semibold text-sm">{title}</h4>
					<p className="text-xs text-muted-foreground truncate">
						{prompt.description || prompt.resource}
					</p>
				</div>
				<div className="flex items-center gap-1 text-xs text-muted-foreground">
					<Clock className="w-3 h-3" />
					<span>{remaining}s</span>
				</div>
			</div>

			{/* Resource */}
			<div className="bg-muted/50 rounded px-3 py-2 mb-3">
				<code className="text-xs break-all">{prompt.resource}</code>
			</div>

			{/* Actions */}
			<div className="flex gap-2">
				<Button
					variant="outline"
					size="sm"
					className="flex-1"
					onClick={() => onRespond("deny")}
				>
					<X className="w-4 h-4 mr-1" />
					Deny
				</Button>
				<Button
					variant="secondary"
					size="sm"
					className="flex-1"
					onClick={() => onRespond("allow_once")}
				>
					<Check className="w-4 h-4 mr-1" />
					Once
				</Button>
				<Button
					variant="default"
					size="sm"
					className="flex-1"
					onClick={() => onRespond("allow_session")}
				>
					<Check className="w-4 h-4 mr-1" />
					Session
				</Button>
			</div>
		</motion.div>
	);
}

// ============================================================================
// Prompt Stack (multiple prompts stacked)
// ============================================================================

interface PromptStackProps {
	prompts: Prompt[];
	onRespond: (promptId: string, action: PromptAction) => void;
}

export function PromptStack({ prompts, onRespond }: PromptStackProps) {
	if (prompts.length === 0) return null;

	return (
		<div className="fixed top-4 right-4 z-50 flex flex-col gap-2">
			<AnimatePresence>
				{prompts.slice(0, 3).map((prompt) => (
					<PromptCard
						key={prompt.id}
						prompt={prompt}
						onRespond={(action) => onRespond(prompt.id, action)}
					/>
				))}
			</AnimatePresence>

			{prompts.length > 3 && (
				<div className="text-xs text-muted-foreground text-center">
					+{prompts.length - 3} more pending
				</div>
			)}
		</div>
	);
}

// ============================================================================
// Full Dialog (for detailed view)
// ============================================================================

interface PromptDialogProps {
	prompt: Prompt | null;
	onRespond: (action: PromptAction) => void;
	onClose: () => void;
}

export function PromptDetailDialog({
	prompt,
	onRespond,
	onClose,
}: PromptDialogProps) {
	const [remaining, setRemaining] = useState(
		prompt ? getRemainingTime(prompt) : 0,
	);

	useEffect(() => {
		if (!prompt) return;

		const interval = setInterval(() => {
			const r = getRemainingTime(prompt);
			setRemaining(r);
			if (r === 0) {
				onClose();
			}
		}, 1000);
		return () => clearInterval(interval);
	}, [prompt, onClose]);

	if (!prompt) return null;

	const icon = prompt.source === "octo_ssh_proxy" ? Key : FileKey;
	const Icon = icon;

	return (
		<Dialog open={!!prompt} onOpenChange={(open) => !open && onClose()}>
			<DialogContent className="sm:max-w-md">
				<DialogHeader>
					<div className="flex items-center gap-3">
						<div className="w-12 h-12 rounded-full bg-amber-500/10 flex items-center justify-center">
							<Icon className="w-6 h-6 text-amber-500" />
						</div>
						<div>
							<DialogTitle>{getPromptTitle(prompt)}</DialogTitle>
							<DialogDescription>
								An agent is requesting access to a protected resource
							</DialogDescription>
						</div>
					</div>
				</DialogHeader>

				<div className="space-y-4 py-4">
					{/* Description */}
					{prompt.description && (
						<p className="text-sm">{prompt.description}</p>
					)}

					{/* Resource */}
					<div className="space-y-2">
						<div className="text-xs font-medium text-muted-foreground uppercase">
							Resource
						</div>
						<div className="bg-muted rounded-md px-3 py-2">
							<code className="text-sm break-all">{prompt.resource}</code>
						</div>
					</div>

					{/* Context */}
					{prompt.context && (
						<div className="space-y-2">
							<div className="text-xs font-medium text-muted-foreground uppercase">
								Details
							</div>
							<pre className="bg-muted rounded-md px-3 py-2 text-xs overflow-auto max-h-32">
								{JSON.stringify(prompt.context, null, 2)}
							</pre>
						</div>
					)}

					{/* Timer */}
					<div className="flex items-center gap-2 text-sm text-muted-foreground">
						<Clock className="w-4 h-4" />
						<span>
							Auto-deny in <strong>{remaining}</strong> seconds
						</span>
					</div>

					{/* Warning */}
					<div className="flex items-start gap-2 p-3 bg-amber-500/10 rounded-md">
						<AlertTriangle className="w-4 h-4 text-amber-500 flex-shrink-0 mt-0.5" />
						<p className="text-xs text-amber-700 dark:text-amber-300">
							Only allow access if you trust the agent and understand why it
							needs this resource.
						</p>
					</div>
				</div>

				<DialogFooter className="flex-col sm:flex-row gap-2">
					<Button
						variant="outline"
						onClick={() => onRespond("deny")}
						className="w-full sm:w-auto"
					>
						<X className="w-4 h-4 mr-2" />
						Deny
					</Button>
					<Button
						variant="secondary"
						onClick={() => onRespond("allow_once")}
						className="w-full sm:w-auto"
					>
						<Check className="w-4 h-4 mr-2" />
						Allow Once
					</Button>
					<Button
						variant="default"
						onClick={() => onRespond("allow_session")}
						className="w-full sm:w-auto"
					>
						<Check className="w-4 h-4 mr-2" />
						Allow for Session
					</Button>
				</DialogFooter>
			</DialogContent>
		</Dialog>
	);
}

// ============================================================================
// Main Prompts Provider Component
// ============================================================================

/**
 * Prompts container component.
 *
 * Add this to your app layout to show security prompts:
 *
 * ```tsx
 * <PromptsContainer />
 * ```
 */
export function PromptsContainer() {
	const { prompts, respond } = usePrompts();
	const [selectedPrompt, setSelectedPrompt] = useState<Prompt | null>(null);

	const handleRespond = async (promptId: string, action: PromptAction) => {
		try {
			await respond(promptId, action);
			if (selectedPrompt?.id === promptId) {
				setSelectedPrompt(null);
			}
		} catch (e) {
			console.error("Failed to respond to prompt:", e);
		}
	};

	return (
		<>
			{/* Stack of prompt cards */}
			<PromptStack prompts={prompts} onRespond={handleRespond} />

			{/* Detailed dialog */}
			<PromptDetailDialog
				prompt={selectedPrompt}
				onRespond={(action) =>
					selectedPrompt && handleRespond(selectedPrompt.id, action)
				}
				onClose={() => setSelectedPrompt(null)}
			/>
		</>
	);
}

// ============================================================================
// Badge for showing pending count
// ============================================================================

interface PromptBadgeProps {
	count: number;
	className?: string;
}

export function PromptBadge({ count, className }: PromptBadgeProps) {
	if (count === 0) return null;

	return (
		<span
			className={cn(
				"inline-flex items-center justify-center",
				"min-w-[18px] h-[18px] px-1",
				"text-[10px] font-medium",
				"bg-amber-500 text-white rounded-full",
				className,
			)}
		>
			{count > 9 ? "9+" : count}
		</span>
	);
}
