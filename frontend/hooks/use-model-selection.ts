/**
 * useModelSelection hook - manages model selection with pending changes
 *
 * Event-driven design:
 * - Fetches available models once per session (no polling)
 * - Derives idle/streaming state from agent events (no polling)
 * - Only polls get_state when a pending model switch needs to be applied
 * - Listens for config.model_changed events to sync state
 */

import { useSelectedChat } from "@/components/contexts";
import type { PiModelInfo } from "@/features/chat/api";
import { updateSettingsValues } from "@/lib/api/settings";
import { normalizeWorkspacePath } from "@/lib/session-utils";
import { getWsManager } from "@/lib/ws-manager";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";

export interface ModelSelectionState {
	/** Available models for the session */
	availableModels: PiModelInfo[];
	/** Currently selected model (provider/model format) */
	selectedModelRef: string | null;
	/** Pending model to apply after streaming (provider/model format) */
	pendingModelRef: string | null;
	/** Whether a model switch is in progress */
	isSwitching: boolean;
	/** Whether models are being loaded */
	loading: boolean;
	/** Whether Pi is idle (not streaming/compacting) */
	isIdle: boolean;
}

export interface ModelSelectionActions {
	/** Select a new model (queues during streaming) */
	selectModel: (modelRef: string) => Promise<void>;
	/** Cycle to next model in scoped list */
	cycleModel: () => Promise<void>;
}

export function useModelSelection(
	sessionId: string | null,
	workspacePath: string | null,
	locale: "en" | "de" = "en",
): ModelSelectionState & ModelSelectionActions {
	const { selectedChatFromHistory } = useSelectedChat();
	const _normalizedWorkspacePath = useMemo(
		() => normalizeWorkspacePath(workspacePath),
		[workspacePath],
	);

	const [availableModels, setAvailableModels] = useState<PiModelInfo[]>([]);
	const [selectedModelRef, setSelectedModelRef] = useState<string | null>(null);
	const [pendingModelRef, setPendingModelRef] = useState<string | null>(null);
	const [isSwitching, setIsSwitching] = useState(false);
	const [loading, setLoading] = useState(false);
	const [isIdle, setIsIdle] = useState(true);

	// Refs to avoid stale closures in event handlers
	const pendingModelRefRef = useRef(pendingModelRef);
	pendingModelRefRef.current = pendingModelRef;

	const effectiveSessionId = sessionId;

	// Storage key for persisting selected model
	const modelStorageKey = useMemo(() => {
		if (!effectiveSessionId) return null;
		return `octo:chatModel:${effectiveSessionId}`;
	}, [effectiveSessionId]);

	// Load selected model: localStorage > hstry session > null
	useEffect(() => {
		if (!modelStorageKey) {
			setSelectedModelRef(null);
			return;
		}
		try {
			const stored = localStorage.getItem(modelStorageKey);
			if (stored) {
				setSelectedModelRef(stored);
				return;
			}
		} catch {
			// ignore localStorage errors
		}
		// Fall back to model/provider from hstry ChatSession
		if (selectedChatFromHistory?.provider && selectedChatFromHistory?.model) {
			const ref = `${selectedChatFromHistory.provider}/${selectedChatFromHistory.model}`;
			setSelectedModelRef(ref);
		} else {
			setSelectedModelRef(null);
		}
	}, [modelStorageKey, selectedChatFromHistory?.provider, selectedChatFromHistory?.model]);

	// Load available models - fetches once per session, retries if Pi not ready yet
	useEffect(() => {
		let active = true;
		let retryTimer: ReturnType<typeof setTimeout> | null = null;

		if (!effectiveSessionId) {
			setAvailableModels([]);
			setLoading(false);
			return undefined;
		}

		const fetchModels = (attempt: number) => {
			if (!active) return;
			if (attempt === 0) setLoading(true);

			getWsManager()
				.agentGetAvailableModels(effectiveSessionId)
				.then((result) => {
					if (!active) return;
					const models = (result as PiModelInfo[]) ?? [];
					if (models.length === 0 && attempt < 5) {
						// Pi process may still be initializing - retry with backoff
						retryTimer = setTimeout(
							() => fetchModels(attempt + 1),
							1000 * (attempt + 1),
						);
						return;
					}
					setAvailableModels(models);
					setLoading(false);
					if (models.length > 0) {
						setSelectedModelRef((prev) => {
							if (prev) return prev;
							const first = models[0];
							return `${first.provider}/${first.id}`;
						});
					}
				})
				.catch(() => {
					if (!active) return;
					if (attempt < 5) {
						retryTimer = setTimeout(
							() => fetchModels(attempt + 1),
							1000 * (attempt + 1),
						);
					} else {
						setAvailableModels([]);
						setLoading(false);
					}
				});
		};

		fetchModels(0);

		return () => {
			active = false;
			if (retryTimer) clearTimeout(retryTimer);
		};
	}, [effectiveSessionId]);

	// Derive idle/streaming state from agent events (no polling)
	// Also handles applying pending model when agent goes idle
	useEffect(() => {
		if (!effectiveSessionId) return undefined;

		const unsubscribe = getWsManager().subscribe("agent", (event) => {
			if (!("channel" in event) || event.channel !== "agent") return;
			if (event.session_id !== effectiveSessionId) return;

			const eventType = event.event as string | undefined;

			// Track streaming/idle state from agent events
			if (eventType === "agent.working" || eventType === "stream.start") {
				setIsIdle(false);
			} else if (
				eventType === "agent.idle" ||
				eventType === "stream.end" ||
				eventType === "agent.error"
			) {
				setIsIdle(true);

				// Apply pending model now that agent is idle
				const pending = pendingModelRefRef.current;
				if (pending && effectiveSessionId) {
					void applyPendingModel(pending, effectiveSessionId, workspacePath);
				}
			}

			// Sync model from config.model_changed events
			if (eventType === "config.model_changed") {
				const provider = event.provider as string | undefined;
				const modelId = event.model_id as string | undefined;
				if (provider && modelId) {
					const newRef = `${provider}/${modelId}`;
					setSelectedModelRef(newRef);
					if (pendingModelRefRef.current === newRef) {
						setPendingModelRef(null);
					}
				}
			}
		});
		return unsubscribe;
	}, [effectiveSessionId, workspacePath]);

	// Fetch initial idle state once on mount (single request, not a poll)
	useEffect(() => {
		if (!effectiveSessionId) {
			setIsIdle(true);
			return;
		}
		getWsManager()
			.agentGetStateWait(effectiveSessionId)
			.then((state) => {
				const s = state as {
					isStreaming: boolean;
					isCompacting: boolean;
				} | null;
				if (s) {
					setIsIdle(!s.isStreaming && !s.isCompacting);
				}
			})
			.catch(() => {
				// Ignore - default to idle
			});
	}, [effectiveSessionId]);

	// Helper to apply a pending model switch
	const applyPendingModel = async (
		modelRef: string,
		sid: string,
		wp: string | null,
	) => {
		setPendingModelRef(null);
		const separatorIndex = modelRef.indexOf("/");
		if (separatorIndex <= 0 || separatorIndex >= modelRef.length - 1) return;

		const provider = modelRef.slice(0, separatorIndex);
		const modelId = modelRef.slice(separatorIndex + 1);
		try {
			await getWsManager().agentSetModel(sid, provider, modelId);
			const settingsWorkspacePath = wp ?? undefined;
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
			console.error("Failed to apply pending model:", err);
		}
	};

	const selectModel = useCallback(
		async (modelRef: string) => {
			if (!modelRef || !effectiveSessionId) return;

			const separatorIndex = modelRef.indexOf("/");
			if (separatorIndex <= 0 || separatorIndex === modelRef.length - 1) {
				return;
			}

			if (isIdle) {
				// Apply immediately if idle
				setSelectedModelRef(modelRef);
				setIsSwitching(true);
				const provider = modelRef.slice(0, separatorIndex);
				const modelId = modelRef.slice(separatorIndex + 1);
				try {
					await getWsManager().agentSetModel(
						effectiveSessionId,
						provider,
						modelId,
					);
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
					setIsSwitching(false);
				}
			} else {
				// Queue for after streaming completes
				setPendingModelRef(modelRef);
			}
		},
		[isIdle, effectiveSessionId, workspacePath],
	);

	const cycleModel = useCallback(async () => {
		if (!effectiveSessionId || availableModels.length === 0) return;

		try {
			await getWsManager().agentCycleModel(effectiveSessionId);
		} catch (err) {
			console.error("Failed to cycle model:", err);
		}
	}, [effectiveSessionId, availableModels.length]);

	return {
		availableModels,
		selectedModelRef,
		pendingModelRef,
		isSwitching,
		loading,
		isIdle,
		selectModel,
		cycleModel,
	};
}
