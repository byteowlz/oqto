"use client";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
	Select,
	SelectContent,
	SelectItem,
	SelectTrigger,
	SelectValue,
} from "@/components/ui/select";
import { Slider } from "@/components/ui/slider";
import {
	type TTSSettings,
	loadTTSSettings,
	saveTTSSettings,
} from "@/features/voice/hooks/useTTS";
import { useModelSelection } from "@/hooks/use-model-selection";
import { triggerChatHistoryBackfill } from "@/lib/api/chat";
import { type ChatVerbosity, useChatVerbosity } from "@/lib/chat-verbosity";
import { fuzzyMatch } from "@/lib/slash-commands";
import { cn } from "@/lib/utils";
import { type WsMuxConnectionState, getWsManager } from "@/lib/ws-manager";
import { Loader2 } from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";

export interface PiSettingsViewProps {
	className?: string;
	locale?: "en" | "de";
	sessionId?: string | null;
	workspacePath?: string | null;
	onHistorySynced?: () => Promise<void> | void;
}

export function PiSettingsView({
	className,
	locale = "en",
	sessionId,
	workspacePath,
	onHistorySynced,
}: PiSettingsViewProps) {
	const { t } = useTranslation();
	const { verbosity, setVerbosity } = useChatVerbosity();
	const [modelQuery, setModelQuery] = useState("");
	const [ttsSettings, setTtsSettings] = useState<TTSSettings>(loadTTSSettings);
	const [thinkingLevel, setThinkingLevel] = useState<string>("off");
	const [thinkingLoading, setThinkingLoading] = useState<boolean>(false);
	const [sessionReady, setSessionReady] = useState<boolean>(false);
	const [restartingAgent, setRestartingAgent] = useState<boolean>(false);
	const [syncingHistory, setSyncingHistory] = useState<boolean>(false);
	const [connectionState, setConnectionState] =
		useState<WsMuxConnectionState>("connecting");
	const [isWorking, setIsWorking] = useState<boolean>(false);
	const [runtimeSessionId, setRuntimeSessionId] = useState<string | null>(null);

	const handleTtsVoiceChange = useCallback(
		(e: React.ChangeEvent<HTMLInputElement>) => {
			const voice = e.target.value;
			const updated = { ...ttsSettings, voice };
			setTtsSettings(updated);
			saveTTSSettings(updated);
		},
		[ttsSettings],
	);

	const handleTtsSpeedChange = useCallback(
		(value: number[]) => {
			const speed = Math.max(0.5, Math.min(2.0, value[0]));
			const updated = { ...ttsSettings, speed };
			setTtsSettings(updated);
			saveTTSSettings(updated);
		},
		[ttsSettings],
	);

	useEffect(() => {
		if (!sessionId) {
			setThinkingLevel("off");
			setThinkingLoading(false);
			setSessionReady(false);
			return;
		}

		let active = true;
		const manager = getWsManager();
		setSessionReady(manager.isSessionReady(sessionId));
		setThinkingLoading(true);

		void manager
			.waitForSessionReady(sessionId, 3000)
			.then(() => {
				if (!active) return;
				setSessionReady(true);
			})
			.catch(() => {
				// Keep false; session can still become ready later via events
			});

		manager
			.agentGetStateWait(sessionId)
			.then((state) => {
				if (!active) return;
				const s = state as {
					thinkingLevel?: string;
					thinking_level?: string;
					sessionId?: string;
					session_id?: string;
				} | null;
				const level = s?.thinkingLevel ?? s?.thinking_level;
				if (typeof level === "string" && level.trim().length > 0) {
					setThinkingLevel(level);
				}
				setRuntimeSessionId(s?.sessionId ?? s?.session_id ?? sessionId);
			})
			.catch(() => {
				// Ignore and keep local fallback
			})
			.finally(() => {
				if (active) setThinkingLoading(false);
			});

		const unsubscribe = manager.subscribe("agent", (event) => {
			if (!active) return;
			if (!("channel" in event) || event.channel !== "agent") return;
			if (event.session_id !== sessionId) return;
			setRuntimeSessionId(event.session_id);
			if (event.event === "session.created") {
				setSessionReady(true);
			}
			if (
				event.event === "config.thinking_level_changed" &&
				typeof event.level === "string"
			) {
				setThinkingLevel(event.level);
			}
			if (
				event.event === "agent.working" ||
				event.event === "stream.message_start"
			) {
				setIsWorking(true);
			}
			if (
				event.event === "agent.idle" ||
				event.event === "agent.error" ||
				event.event === "stream.done"
			) {
				setIsWorking(false);
			}
		});

		return () => {
			active = false;
			unsubscribe();
		};
	}, [sessionId]);

	useEffect(() => {
		const manager = getWsManager();
		const unsubscribe = manager.onConnectionState((state) => {
			setConnectionState(state);
		});
		return unsubscribe;
	}, []);

	const handleThinkingLevelChange = useCallback(
		async (value: string) => {
			if (!sessionId || !value || !sessionReady) return;
			const previous = thinkingLevel;
			setThinkingLevel(value);
			setThinkingLoading(true);
			try {
				const manager = getWsManager();
				await manager.waitForSessionReady(sessionId, 3000);
				await manager.agentSetThinkingLevel(sessionId, value);
				const state = (await manager.agentGetStateWait(sessionId)) as {
					thinkingLevel?: string;
					thinking_level?: string;
				} | null;
				const confirmed = state?.thinkingLevel ?? state?.thinking_level;
				if (typeof confirmed === "string" && confirmed.trim().length > 0) {
					setThinkingLevel(confirmed);
				}
			} catch {
				setThinkingLevel(previous);
			} finally {
				setThinkingLoading(false);
			}
		},
		[sessionId, sessionReady, thinkingLevel],
	);

	const handleRestartAgent = useCallback(async () => {
		if (!sessionId) return;
		setRestartingAgent(true);
		setSessionReady(false);
		try {
			const manager = getWsManager();
			await manager.agentRestartSession(sessionId);
			await manager.waitForSessionReady(sessionId, 10000);
			setSessionReady(true);
			const state = (await manager.agentGetStateWait(sessionId)) as {
				thinkingLevel?: string;
				thinking_level?: string;
			} | null;
			const level = state?.thinkingLevel ?? state?.thinking_level;
			if (typeof level === "string" && level.trim().length > 0) {
				setThinkingLevel(level);
			}
		} finally {
			setRestartingAgent(false);
		}
	}, [sessionId]);

	const handleSyncChatHistory = useCallback(async () => {
		setSyncingHistory(true);
		try {
			const result = await triggerChatHistoryBackfill(
				workspacePath ? { workspace: workspacePath } : {},
			);
			if (onHistorySynced) {
				await onHistorySynced();
			}
			if (sessionId) {
				const manager = getWsManager();
				manager.agentGetMessages(sessionId);
			}
			toast.success(
				`Sync complete: repaired ${result.repaired_conversations}, scanned ${result.scanned_files}`,
			);
		} catch (err) {
			toast.error(err instanceof Error ? err.message : "History sync failed");
		} finally {
			setSyncingHistory(false);
		}
	}, [workspacePath, onHistorySynced, sessionId]);

	const {
		availableModels,
		selectedModelRef,
		pendingModelRef,
		isSwitching,
		loading,
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

	const verbosityLabel = t("pi.chatVerbosity");
	const verbosityDescription = t("pi.chatVerbosityDescription");

	const handleModelChange = useCallback(
		async (value: string) => {
			if (!value) return;
			await selectModel(value);
			setModelQuery("");
		},
		[selectModel],
	);

	const effectiveModelRef = pendingModelRef ?? selectedModelRef;
	const wsLabel =
		connectionState === "connected"
			? "up"
			: connectionState === "disconnected"
				? "down"
				: "connecting";
	const agentLabel = isWorking ? "working" : "idle";
	const viewSessionShort = sessionId ? sessionId.slice(0, 8) : "none";
	const runtimeSessionShort = runtimeSessionId
		? runtimeSessionId.slice(0, 8)
		: "none";
	const sessionMismatch =
		!!sessionId && !!runtimeSessionId && sessionId !== runtimeSessionId;

	return (
		<div className={cn("flex flex-col h-full", className)}>
			<div className="flex items-center justify-between p-3 border-b border-border">
				<span className="text-sm font-medium">{t("pi.settings")}</span>
			</div>
			<div className="flex-1 overflow-auto p-3 space-y-5">
				<div className="rounded border border-border/70 bg-muted/30 px-2 py-1.5 text-[11px] text-muted-foreground">
					<div className="flex flex-wrap items-center gap-x-3 gap-y-1">
						<span
							className={cn(
								"inline-flex items-center rounded border px-1.5 py-0.5 font-mono",
								wsLabel === "up"
									? "border-emerald-500/40 text-emerald-600 dark:text-emerald-400"
									: "border-destructive/40 text-destructive",
							)}
						>
							ws:{wsLabel}
						</span>
						<span className="font-mono">agent:{agentLabel}</span>
						<span className="font-mono">view:{viewSessionShort}</span>
						<span className="font-mono">runtime:{runtimeSessionShort}</span>
						{sessionMismatch && (
							<span className="inline-flex items-center rounded border border-amber-500/40 bg-amber-500/10 px-1.5 py-0.5 font-mono text-amber-700 dark:text-amber-300">
								session mismatch
							</span>
						)}
					</div>
				</div>

				<div className="space-y-2">
					<Label className="text-xs font-medium">{t("models.model")}</Label>
					{loading ? (
						<div className="flex items-center gap-2 text-xs text-muted-foreground">
							<Loader2 className="h-4 w-4 animate-spin" />
							{t("models.loadingModels")}
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
											? t("models.switchingModel")
											: t("models.selectModel")
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
										placeholder={t("models.searchModels")}
										value={modelQuery}
										onChange={(e) => setModelQuery(e.target.value)}
										className="h-8"
									/>
								</div>
								{availableModels.length === 0 ? (
									<div className="p-3 text-sm text-muted-foreground text-center">
										{t("models.noModelsAvailable")}
									</div>
								) : filteredModels.length === 0 ? (
									<div className="p-3 text-sm text-muted-foreground text-center">
										{t("models.noMatches")}
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
															{t("models.pending")}
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
							{t("models.modelChangeAfterCompletion")}
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
							<SelectItem value="1">{t("pi.minimal")}</SelectItem>
							<SelectItem value="2">{t("pi.compact")}</SelectItem>
							<SelectItem value="3">{t("pi.verbose")}</SelectItem>
						</SelectContent>
					</Select>
					<p className="text-[10px] text-muted-foreground">
						{verbosityDescription}
					</p>
				</div>

				<div className="space-y-2">
					<Label className="text-xs font-medium text-muted-foreground">
						{t("pi.reasoningLevel")}
					</Label>
					<Select
						value={thinkingLevel}
						onValueChange={handleThinkingLevelChange}
						disabled={!sessionId || !sessionReady || restartingAgent}
					>
						<SelectTrigger className="w-full">
							<SelectValue
								placeholder={
									thinkingLoading
										? t("models.loadingModels")
										: t("pi.selectReasoningLevel")
								}
							/>
						</SelectTrigger>
						<SelectContent>
							<SelectItem value="off">{t("pi.thinkingOff")}</SelectItem>
							<SelectItem value="minimal">{t("pi.thinkingMinimal")}</SelectItem>
							<SelectItem value="low">{t("pi.thinkingLow")}</SelectItem>
							<SelectItem value="medium">{t("pi.thinkingMedium")}</SelectItem>
							<SelectItem value="high">{t("pi.thinkingHigh")}</SelectItem>
							<SelectItem value="xhigh">{t("pi.thinkingXHigh")}</SelectItem>
						</SelectContent>
					</Select>
					<p className="text-[10px] text-muted-foreground">
						{sessionId && sessionReady
							? t("pi.reasoningLevelDescription")
							: t("pi.reasoningLevelRequiresSession")}
					</p>
				</div>

				<div className="space-y-2">
					<Label className="text-xs font-medium text-muted-foreground">
						{t("pi.agentControl")}
					</Label>
					<Button
						type="button"
						variant="outline"
						size="sm"
						className="w-full"
						onClick={handleRestartAgent}
						disabled={!sessionId || restartingAgent}
					>
						{restartingAgent ? (
							<span className="inline-flex items-center gap-2">
								<Loader2 className="h-3 w-3 animate-spin" />
								{t("pi.restartingAgent")}
							</span>
						) : (
							t("pi.restartAgent")
						)}
					</Button>
					<Button
						type="button"
						variant="outline"
						size="sm"
						className="w-full"
						onClick={handleSyncChatHistory}
						disabled={syncingHistory}
					>
						{syncingHistory ? (
							<span className="inline-flex items-center gap-2">
								<Loader2 className="h-3 w-3 animate-spin" />
								Syncing history...
							</span>
						) : (
							"Sync chat history"
						)}
					</Button>
					<p className="text-[10px] text-muted-foreground">
						{t("pi.restartAgentDescription")}
					</p>
					<p className="text-[10px] text-muted-foreground">
						Scan Pi JSONL and reconcile missing messages into hstry.
					</p>
				</div>

				{/* Read Aloud (TTS) settings */}
				<div className="space-y-3">
					<Label className="text-xs font-medium text-muted-foreground">
						{t("pi.readAloud", "Read Aloud")}
					</Label>

					<div className="space-y-1.5">
						<Label className="text-[11px] text-muted-foreground">
							{t("pi.ttsVoice", "Voice")}
						</Label>
						<Input
							value={ttsSettings.voice}
							onChange={handleTtsVoiceChange}
							placeholder="af_heart"
							className="h-8 text-xs"
						/>
					</div>

					<div className="space-y-1.5">
						<div className="flex items-center justify-between">
							<Label className="text-[11px] text-muted-foreground">
								{t("pi.ttsSpeed", "Speed")}
							</Label>
							<span className="text-[11px] text-muted-foreground tabular-nums">
								{ttsSettings.speed.toFixed(1)}x
							</span>
						</div>
						<Slider
							value={[ttsSettings.speed]}
							onValueChange={handleTtsSpeedChange}
							min={0.5}
							max={2.0}
							step={0.1}
						/>
					</div>

					<p className="text-[10px] text-muted-foreground">
						{t(
							"pi.ttsDescription",
							"Voice and speed for the Read Aloud button on messages.",
						)}
					</p>
				</div>
			</div>
		</div>
	);
}
