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
import {
	type PiModelInfo,
	getMainChatPiModels,
	setMainChatPiModel,
} from "@/features/main-chat/api";
import { fuzzyMatch } from "@/lib/slash-commands";
import { cn } from "@/lib/utils";
import { Loader2 } from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";

interface MainChatSettingsViewProps {
	className?: string;
	locale?: "en" | "de";
}

export function MainChatSettingsView({
	className,
	locale = "en",
}: MainChatSettingsViewProps) {
	const [availableModels, setAvailableModels] = useState<PiModelInfo[]>([]);
	const [selectedModelRef, setSelectedModelRef] = useState<string | null>(null);
	const [isSwitchingModel, setIsSwitchingModel] = useState(false);
	const [modelQuery, setModelQuery] = useState("");
	const [loading, setLoading] = useState(true);

	// Load available models
	useEffect(() => {
		let active = true;
		setLoading(true);
		getMainChatPiModels()
			.then((models) => {
				if (active) {
					setAvailableModels(models);
					// Set initial selection to first model if not set
					if (models.length > 0 && !selectedModelRef) {
						const firstModel = models[0];
						setSelectedModelRef(`${firstModel.provider}/${firstModel.id}`);
					}
				}
			})
			.catch(() => {
				if (active) setAvailableModels([]);
			})
			.finally(() => {
				if (active) setLoading(false);
			});
		return () => {
			active = false;
		};
	}, [selectedModelRef]);

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

	const handleModelChange = useCallback(async (value: string) => {
		const separatorIndex = value.indexOf("/");
		if (separatorIndex <= 0 || separatorIndex === value.length - 1) return;
		const provider = value.slice(0, separatorIndex);
		const modelId = value.slice(separatorIndex + 1);
		setSelectedModelRef(value);
		setIsSwitchingModel(true);
		try {
			await setMainChatPiModel(provider, modelId);
		} catch (err) {
			console.error("Failed to switch model:", err);
		} finally {
			setIsSwitchingModel(false);
		}
	}, []);

	if (loading) {
		return (
			<div
				className={cn("flex items-center justify-center h-full p-4", className)}
			>
				<Loader2 className="h-6 w-6 animate-spin text-muted-foreground" />
			</div>
		);
	}

	return (
		<div className={cn("flex flex-col h-full", className)}>
			{/* Header */}
			<div className="flex items-center justify-between p-3 border-b border-border">
				<span className="text-sm font-medium">
					{locale === "de" ? "Hauptchat Einstellungen" : "Main Chat Settings"}
				</span>
			</div>

			{/* Settings form */}
			<div className="flex-1 overflow-auto p-3 space-y-4">
				{/* Model selector */}
				<div className="space-y-2">
					<Label className="text-xs font-medium">
						{locale === "de" ? "Modell" : "Model"}
					</Label>
					<Select
						value={selectedModelRef ?? undefined}
						onValueChange={handleModelChange}
						onOpenChange={(open) => {
							if (open) setModelQuery("");
						}}
						disabled={isSwitchingModel || availableModels.length === 0}
					>
						<SelectTrigger className="h-8 text-xs">
							<SelectValue
								placeholder={
									isSwitchingModel
										? locale === "de"
											? "Wechsle Modell..."
											: "Switching model..."
										: locale === "de"
											? "Modell auswahlen"
											: "Select model"
								}
							/>
						</SelectTrigger>
						<SelectContent>
							<div
								className="sticky top-0 z-10 bg-popover p-2 border-b border-border"
								onPointerDown={(e) => e.stopPropagation()}
								onKeyDown={(e) => e.stopPropagation()}
							>
								<Input
									value={modelQuery}
									onChange={(e) => setModelQuery(e.target.value)}
									placeholder={
										locale === "de"
											? "Modelle durchsuchen..."
											: "Search models..."
									}
									aria-label={
										locale === "de" ? "Modelle durchsuchen" : "Search models"
									}
									className="h-8 text-xs"
								/>
							</div>
							{availableModels.length === 0 ? (
								<SelectItem value="__none__" disabled>
									{locale === "de"
										? "Keine Modelle verfugbar"
										: "No models available"}
								</SelectItem>
							) : filteredModels.length === 0 ? (
								<SelectItem value="__no_results__" disabled>
									{locale === "de" ? "Keine Treffer" : "No matches"}
								</SelectItem>
							) : (
								filteredModels.map((model) => {
									const value = `${model.provider}/${model.id}`;
									return (
										<SelectItem key={value} value={value}>
											{model.name ? `${value} - ${model.name}` : value}
										</SelectItem>
									);
								})
							)}
						</SelectContent>
					</Select>
					<p className="text-[10px] text-muted-foreground">
						{locale === "de"
							? "Provider/Modell fur den Hauptchat"
							: "Provider/model for the main chat"}
					</p>
				</div>
			</div>
		</div>
	);
}
