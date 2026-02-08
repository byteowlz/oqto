/**
 * ModelQuickSwitcher - quick model switcher with fuzzy search
 *
 * Shows as a popover when clicking model name in status bar.
 * Uses cmdk for fuzzy filtering.
 */

import { ProviderIcon } from "@/components/data-display";
import {
	Command,
	CommandEmpty,
	CommandGroup,
	CommandInput,
	CommandItem,
	CommandList,
} from "@/components/ui/command";
import {
	Popover,
	PopoverContent,
	PopoverTrigger,
} from "@/components/ui/popover";
import type { PiModelInfo } from "@/features/chat/api";
import {
	type ModelSelectionState,
	useModelSelection,
} from "@/hooks/use-model-selection";
import { fuzzyMatch } from "@/lib/slash-commands";
import { cn } from "@/lib/utils";
import { Loader2 } from "lucide-react";
import { useCallback, useMemo, useState } from "react";

interface ModelQuickSwitcherProps {
	/** Locale for UI text */
	locale?: "en" | "de";
	/** Session ID */
	sessionId: string | null;
	/** Workspace path for settings persistence */
	workspacePath: string | null;
	/** Optional additional CSS class name */
	className?: string;
}

export function ModelQuickSwitcher({
	locale = "en",
	sessionId,
	workspacePath,
	className,
}: ModelQuickSwitcherProps) {
	const [open, setOpen] = useState(false);
	const [searchQuery, setSearchQuery] = useState("");

	const {
		availableModels,
		selectedModelRef,
		pendingModelRef,
		isSwitching,
		loading,
		isIdle,
		selectModel,
	} = useModelSelection(sessionId, workspacePath, locale);

	// Filter models with fuzzy search
	const filteredModels = useMemo(() => {
		const query = searchQuery.trim().toLowerCase();
		if (!query) return availableModels;

		return availableModels.filter((model) => {
			const fullRef = `${model.provider}/${model.id}`;
			return (
				fuzzyMatch(query, fullRef) ||
				fuzzyMatch(query, model.provider) ||
				fuzzyMatch(query, model.id) ||
				(model.name ? fuzzyMatch(query, model.name) : false)
			);
		});
	}, [availableModels, searchQuery]);

	// Open settings if "open-settings" is typed
	const handleCommandSubmit = useCallback(
		async (modelRef: string) => {
			setOpen(false);
			setSearchQuery("");
			await selectModel(modelRef);
		},
		[selectModel],
	);

	const currentModelLabel = useMemo(() => {
		if (pendingModelRef) {
			const pendingModel = availableModels.find(
				(m) => `${m.provider}/${m.id}` === pendingModelRef,
			);
			if (pendingModel) {
				return pendingModel.name
					? `${pendingModelRef} - ${pendingModel.name}`
					: pendingModelRef;
			}
		}

		if (!selectedModelRef) return null;

		const selectedModel = availableModels.find(
			(m) => `${m.provider}/${m.id}` === selectedModelRef,
		);
		if (selectedModel) {
			return selectedModel.name
				? `${selectedModelRef} - ${selectedModel.name}`
				: selectedModelRef;
		}
		return selectedModelRef;
	}, [selectedModelRef, pendingModelRef, availableModels]);

	// Provider from current selection
	const provider = selectedModelRef?.split("/")[0] ?? null;

	// Labels
	const placeholder = locale === "de" ? "Modell suchen..." : "Search models...";
	const emptyState = locale === "de" ? "Keine Treffer" : "No matches";
	const loadingText = locale === "de" ? "Lade Modelle..." : "Loading models...";
	const selectText = locale === "de" ? "Modell auswählen" : "Select a model";
	const queueText =
		locale === "de"
			? "(Wird nach Abschluss angewendet)"
			: "(Will apply after completion)";
	const pendingText = locale === "de" ? "Ausstehend:" : "Pending:";

	return (
		<Popover open={open} onOpenChange={setOpen}>
			<PopoverTrigger asChild>
				<button
					type="button"
					className={cn(
						"flex items-center gap-1 hover:bg-accent hover:text-accent-foreground rounded px-1.5 transition-colors",
						className,
					)}
					onClick={() => setOpen((prev) => !prev)}
				>
					{provider && <ProviderIcon provider={provider} className="w-3 h-3" />}
					<span className="font-mono">{currentModelLabel}</span>
					{pendingModelRef && (
						<span className="text-xs text-muted-foreground ml-1">
							{queueText}
						</span>
					)}
					{isSwitching && <Loader2 className="w-3 h-3 animate-spin" />}
				</button>
			</PopoverTrigger>
			<PopoverContent
				className="w-[360px] p-0"
				align="start"
				side="top"
				onOpenAutoFocus={(e) => e.preventDefault()}
			>
				<Command
					className="rounded-lg border shadow-md"
					shouldFilter={false}
					onValueChange={(value) => {
						if (value) {
							void handleCommandSubmit(value);
						}
					}}
				>
					{!loading && availableModels.length > 0 && (
						<CommandInput
							placeholder={placeholder}
							value={searchQuery}
							onValueChange={setSearchQuery}
						/>
					)}
					<CommandList>
						{loading ? (
							<div className="py-6 flex items-center justify-center text-sm text-muted-foreground">
								<Loader2 className="w-4 h-4 mr-2 animate-spin" />
								{loadingText}
							</div>
						) : availableModels.length === 0 ? (
							<CommandEmpty>{selectText}</CommandEmpty>
						) : filteredModels.length === 0 ? (
							<CommandEmpty>{emptyState}</CommandEmpty>
						) : (
							<CommandGroup>
								{filteredModels.map((model) => {
									const value = `${model.provider}/${model.id}`;
									const isSelected = value === selectedModelRef;
									const isPending = value === pendingModelRef;

									return (
										<CommandItem
											key={value}
											value={value}
											onSelect={() => {
												void handleCommandSubmit(value);
											}}
										>
											<ProviderIcon
												provider={model.provider}
												className="w-4 h-4 mr-2"
											/>
											<div className="flex-1 min-w-0">
												<div className="font-mono text-sm">{value}</div>
												{model.name && (
													<div className="text-xs text-muted-foreground truncate">
														{model.name}
													</div>
												)}
											</div>
											{isSelected && !isIdle && (
												<span className="text-[10px] text-muted-foreground">
													{queueText}
												</span>
											)}
											{isPending && (
												<span className="text-[10px] text-muted-foreground font-medium">
													{pendingText}
												</span>
											)}
										</CommandItem>
									);
								})}
							</CommandGroup>
						)}
					</CommandList>
				</Command>
			</PopoverContent>
		</Popover>
	);
}
