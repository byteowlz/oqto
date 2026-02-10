"use client";

import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
	Select,
	SelectContent,
	SelectItem,
	SelectTrigger,
	SelectValue,
} from "@/components/ui/select";
import { useModelSelection } from "@/hooks/use-model-selection";
import { type ChatVerbosity, useChatVerbosity } from "@/lib/chat-verbosity";
import { fuzzyMatch } from "@/lib/slash-commands";
import { cn } from "@/lib/utils";
import { Loader2 } from "lucide-react";
import { useCallback, useMemo, useState } from "react";

export interface PiSettingsViewProps {
	className?: string;
	locale?: "en" | "de";
	sessionId?: string | null;
	workspacePath?: string | null;
}

export function PiSettingsView({
	className,
	locale = "en",
	sessionId,
	workspacePath,
}: PiSettingsViewProps) {
	const { verbosity, setVerbosity } = useChatVerbosity();
	const [modelQuery, setModelQuery] = useState("");

	const {
		availableModels,
		selectedModelRef,
		pendingModelRef,
		isSwitching,
		loading,
		isIdle,
		selectModel,
	} = useModelSelection(sessionId, workspacePath, locale);

	const filteredModels = useMemo(() => {
		const query = modelQuery.trim();
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
	}, [availableModels, modelQuery]);

	const verbosityLabel = locale === "de" ? "Chat-Detailgrad" : "Chat verbosity";
	const verbosityDescription =
		locale === "de"
			? "Steuert, wie detailliert Tool-Aufrufe angezeigt werden."
			: "Controls how detailed tool call rendering is.";

	const handleModelChange = useCallback(
		async (value: string) => {
			if (!value) return;
			await selectModel(value);
			setModelQuery("");
		},
		[selectModel],
	);

	const effectiveModelRef = pendingModelRef ?? selectedModelRef;

	return (
		<div className={cn("flex flex-col h-full", className)}>
			<div className="flex items-center justify-between p-3 border-b border-border">
				<span className="text-sm font-medium">
					{locale === "de" ? "Pi Einstellungen" : "Pi Settings"}
				</span>
			</div>
			<div className="flex-1 overflow-auto p-3 space-y-5">
				<div className="space-y-2">
					<Label className="text-xs font-medium">
						{locale === "de" ? "Modell" : "Model"}
					</Label>
					{loading ? (
						<div className="flex items-center gap-2 text-xs text-muted-foreground">
							<Loader2 className="h-4 w-4 animate-spin" />
							{locale === "de" ? "Modelle laden..." : "Loading models..."}
						</div>
					) : (
						<Select
							value={effectiveModelRef ?? undefined}
							onValueChange={handleModelChange}
							disabled={isSwitching || availableModels.length === 0}
							onOpenChange={(open) => {
								if (open) setModelQuery("");
							}}
						>
							<SelectTrigger className="w-full">
								<SelectValue
									placeholder={
										isSwitching
											? locale === "de"
												? "Wechsle Modell..."
												: "Switching model..."
											: locale === "de"
												? "Modell auswählen"
												: "Select model"
									}
								/>
							</SelectTrigger>
							<SelectContent className="w-[320px]">
								<div
									className="p-2 border-b border-border"
									onPointerDown={(e) => e.stopPropagation()}
									onKeyDown={(e) => e.stopPropagation()}
								>
									<Input
										placeholder={
											locale === "de"
												? "Modelle durchsuchen..."
												: "Search models..."
										}
										value={modelQuery}
										onChange={(e) => setModelQuery(e.target.value)}
										className="h-8"
									/>
								</div>
								{availableModels.length === 0 ? (
									<div className="p-3 text-sm text-muted-foreground text-center">
										{locale === "de"
											? "Keine Modelle verfügbar"
											: "No models available"}
									</div>
								) : filteredModels.length === 0 ? (
									<div className="p-3 text-sm text-muted-foreground text-center">
										{locale === "de" ? "Keine Treffer" : "No matches"}
									</div>
								) : (
									filteredModels.map((model) => {
										const value = `${model.provider}/${model.id}`;
										const isSelected = value === selectedModelRef;
										const isPending = value === pendingModelRef;

										return (
											<SelectItem key={value} value={value}>
												<div className="flex items-center gap-2">
													<span className="flex-1">
														{model.name ? `${value} - ${model.name}` : value}
													</span>
													{isPending && (
														<span className="text-[10px] text-muted-foreground">
															{locale === "de"
																? "(Wird angewendet)"
																: "(Pending)"}
														</span>
													)}
												</div>
											</SelectItem>
										);
									})
								)}
							</SelectContent>
						</Select>
					)}
					{pendingModelRef && (
						<p className="text-[10px] text-muted-foreground">
							{locale === "de"
								? "Modellwechsel wird nach Abschluss angewendet."
								: "Model change will apply after completion."}
						</p>
					)}
				</div>

				<div className="space-y-2">
					<Label className="text-xs font-medium text-muted-foreground">
						{verbosityLabel}
					</Label>
					<Select
						value={String(verbosity)}
						onValueChange={(value) =>
							setVerbosity(Number(value) as ChatVerbosity)
						}
					>
						<SelectTrigger className="w-full">
							<SelectValue />
						</SelectTrigger>
						<SelectContent>
							<SelectItem value="1">
								{locale === "de" ? "Minimal" : "Minimal"}
							</SelectItem>
							<SelectItem value="2">
								{locale === "de" ? "Kompakt" : "Compact"}
							</SelectItem>
							<SelectItem value="3">
								{locale === "de" ? "Ausführlich" : "Verbose"}
							</SelectItem>
						</SelectContent>
					</Select>
					<p className="text-[10px] text-muted-foreground">
						{verbosityDescription}
					</p>
				</div>
			</div>
		</div>
	);
}
