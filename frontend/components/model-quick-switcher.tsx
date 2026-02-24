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
import { Clock, Loader2 } from "lucide-react";
import { useCallback, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";

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
	const { t } = useTranslation();
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

	const modelLabel = useMemo(() => {
		const ref = pendingModelRef || selectedModelRef;
		if (!ref) {
			return { full: null, provider: null, model: null };
		}
		const parts = ref.split("/");
		if (parts.length < 2) {
			return { full: ref, provider: null, model: ref };
		}
		return {
			full: ref,
			provider: parts[0],
			model: parts.slice(1).join("/"),
		};
	}, [selectedModelRef, pendingModelRef]);

	// Provider from current selection
	const provider = modelLabel.provider;

	// Labels
	const placeholder = t("models.searchModels");
	const emptyState = t("models.noMatches");
	const loadingText = t("models.loadingModels");
	const selectText = t("models.selectModel");
	const queueText = t("models.queueText");
	const pendingText = t("models.pendingLabel");

	return (
		<Popover open={open} onOpenChange={setOpen}>
			<PopoverTrigger asChild>
				<button
					type="button"
					className={cn(
						"flex items-center gap-1 text-left hover:bg-accent hover:text-accent-foreground rounded px-1.5 transition-colors",
						className,
					)}
					onClick={() => setOpen((prev) => !prev)}
				>
					{provider && <ProviderIcon provider={provider} className="w-3 h-3" />}
					{modelLabel.full ? (
						<>
							<span className="font-mono hidden md:inline">
								{modelLabel.full}
							</span>
							<span className="font-mono flex flex-col text-left leading-tight md:hidden min-w-0">
								{modelLabel.provider ? (
									<>
										<span className="truncate">{modelLabel.provider}</span>
										<span className="truncate">{modelLabel.model}</span>
									</>
								) : (
									<span className="truncate">{modelLabel.model}</span>
								)}
							</span>
						</>
					) : (
						<span className="font-mono text-muted-foreground">
							{loading ? loadingText : selectText}
						</span>
					)}
					{pendingModelRef && (
						<>
							<Clock
								className="w-3 h-3 text-muted-foreground ml-1 md:hidden"
								aria-label={queueText}
							/>
							<span className="text-[10px] text-muted-foreground ml-1 hidden md:inline">
								{queueText}
							</span>
						</>
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
