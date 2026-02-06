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
import type { AgentState, PiModelInfo } from "@/features/chat/api";
import { updateSettingsValues } from "@/lib/api/settings";
import { type ChatVerbosity, useChatVerbosity } from "@/lib/chat-verbosity";
import { normalizeWorkspacePath } from "@/lib/session-utils";
import { fuzzyMatch } from "@/lib/slash-commands";
import { cn } from "@/lib/utils";
import { getWsManager } from "@/lib/ws-manager";
import { Loader2 } from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";

interface PiSettingsViewProps {
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
	const [availableModels, setAvailableModels] = useState<PiModelInfo[]>([]);
	const [selectedModelRef, setSelectedModelRef] = useState<string | null>(null);
	const [isSwitchingModel, setIsSwitchingModel] = useState(false);
	const [modelQuery, setModelQuery] = useState("");
	const [loadingModels, setLoadingModels] = useState(false);
	const [piState, setPiState] = useState<AgentState | null>(null);
	const [loadingState, setLoadingState] = useState(false);
	const [effectiveSessionId, setEffectiveSessionId] = useState<string | null>(
		sessionId ?? null,
	);
	const normalizedWorkspacePath = useMemo(
		() => normalizeWorkspacePath(workspacePath),
		[workspacePath],
	);

	useEffect(() => {
		setEffectiveSessionId(sessionId ?? null);
	}, [sessionId]);

	const modelStorageKey = useMemo(() => {
		if (!effectiveSessionId) return null;
		return `octo:chatModel:${effectiveSessionId}`;
	}, [effectiveSessionId]);

	useEffect(() => {
		if (!modelStorageKey) {
			setSelectedModelRef(null);
			return;
		}
		try {
			const stored = localStorage.getItem(modelStorageKey);
			setSelectedModelRef(stored);
		} catch {
			setSelectedModelRef(null);
		}
	}, [modelStorageKey]);

	useEffect(() => {
		let active = true;
		if (!effectiveSessionId) return undefined;
		setLoadingModels(true);
		getWsManager()
			.agentGetAvailableModels(effectiveSessionId)
			.then((result) => {
				if (!active) return;
				const models = (result as PiModelInfo[]) ?? [];
				setAvailableModels(models);
				if (!selectedModelRef && models.length > 0) {
					const first = models[0];
					setSelectedModelRef(`${first.provider}/${first.id}`);
				}
			})
			.catch(() => {
				if (active) setAvailableModels([]);
			})
			.finally(() => {
				if (active) setLoadingModels(false);
			});
		return () => {
			active = false;
		};
	}, [effectiveSessionId, selectedModelRef]);

	useEffect(() => {
		let active = true;
		let intervalId: ReturnType<typeof setInterval> | null = null;
		const fetchState = async () => {
			if (!active) return;
			try {
				const nextState = effectiveSessionId
					? ((await getWsManager().agentGetStateWait(
							effectiveSessionId,
						)) as AgentState | null)
					: null;
				if (active) {
					setPiState(nextState);
				}
			} catch {
				if (active) setPiState(null);
			} finally {
				if (active) setLoadingState(false);
			}
		};
		if (effectiveSessionId) {
			setLoadingState(true);
			void fetchState();
			intervalId = setInterval(fetchState, 2000);
		} else {
			setPiState(null);
			setLoadingState(false);
		}
		return () => {
			active = false;
			if (intervalId) clearInterval(intervalId);
		};
	}, [effectiveSessionId]);

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

	const isIdle = !(piState?.isStreaming || piState?.isCompacting);

	const verbosityLabel = locale === "de" ? "Chat-Detailgrad" : "Chat verbosity";
	const verbosityDescription =
		locale === "de"
			? "Steuert, wie detailliert Tool-Aufrufe angezeigt werden."
			: "Controls how detailed tool call rendering is.";

	const handleModelChange = useCallback(
		async (value: string) => {
			if (!isIdle) {
				return;
			}
			if (!value) return;
			const separatorIndex = value.indexOf("/");
			if (separatorIndex <= 0 || separatorIndex === value.length - 1) return;
			const provider = value.slice(0, separatorIndex);
			const modelId = value.slice(separatorIndex + 1);
			setSelectedModelRef(value);
			setIsSwitchingModel(true);
			try {
				const sessionId = effectiveSessionId;
				if (!sessionId) {
					throw new Error("No active chat session -- send a message first");
				}
				await getWsManager().agentSetModel(sessionId, provider, modelId);
				// Persist defaults once the user changes the model.
				const settingsWorkspacePath = workspacePath ?? undefined;
				await updateSettingsValues(
					"pi-agent",
					{
						values: {
							defaultProvider: provider,
							defaultModel: modelId,
						},
					},
					settingsWorkspacePath,
				);
			} catch (err) {
				console.error("Failed to switch model:", err);
			} finally {
				setIsSwitchingModel(false);
			}
		},
		[isIdle, effectiveSessionId, workspacePath],
	);

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
					{loadingModels ? (
						<div className="flex items-center gap-2 text-xs text-muted-foreground">
							<Loader2 className="h-4 w-4 animate-spin" />
							{locale === "de" ? "Modelle laden..." : "Loading models..."}
						</div>
					) : !sessionId ? (
						<p className="text-xs text-muted-foreground">
							{locale === "de"
								? "Keine Sitzung ausgewählt"
								: "No session selected"}
						</p>
					) : (
						<Select
							value={selectedModelRef ?? undefined}
							onValueChange={handleModelChange}
							disabled={
								isSwitchingModel ||
								availableModels.length === 0 ||
								loadingState ||
								!isIdle
							}
							onOpenChange={(open) => {
								if (open) setModelQuery("");
							}}
						>
							<SelectTrigger className="w-full">
								<SelectValue
									placeholder={
										isSwitchingModel
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
										return (
											<SelectItem key={value} value={value}>
												{model.name ? `${value} - ${model.name}` : value}
											</SelectItem>
										);
									})
								)}
							</SelectContent>
						</Select>
					)}
					{!isIdle && (
						<p className="text-[10px] text-muted-foreground">
							{locale === "de"
								? "Modellwechsel nur im Leerlauf möglich."
								: "Model switching is only available when Pi is idle."}
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
