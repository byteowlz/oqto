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
import {
	getRunnerHistoryAlias,
	useChatContext,
} from "@/components/contexts/chat-context";
import type { PiModelInfo } from "@/features/chat/api";
import { updateSettingsValues } from "@/lib/api/settings";
import {
	getWorkspaceModelStorageKey,
	normalizeWorkspacePath,
} from "@/lib/session-utils";
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

const DEFAULT_REASONING_THINKING_LEVEL = "minimal";
const MODEL_SELECTION_SYNC_EVENT = "oqto:model-selection-sync";
const OAUTH_MODELS_INVALIDATE_EVENT = "oqto:oauth-models-invalidate";

type ModelSelectionSyncDetail = {
	sessionId: string | null;
	selectedModelRef?: string | null;
	pendingModelRef?: string | null;
	isSwitching?: boolean;
	source?: string;
};

function emitModelSelectionSync(detail: ModelSelectionSyncDetail): void {
	if (typeof window === "undefined") return;
	window.dispatchEvent(
		new CustomEvent<ModelSelectionSyncDetail>(MODEL_SELECTION_SYNC_EVENT, {
			detail,
		}),
	);
}

function isReasoningModel(modelRef: string, models: PiModelInfo[]): boolean {
	const [provider, ...modelParts] = modelRef.split("/");
	const modelId = modelParts.join("/");
	if (!provider || !modelId) return false;

	const direct = models.find(
		(m) => m.provider === provider && m.id === modelId,
	);
	if (direct?.reasoning === true) return true;

	const normalizedProvider = provider.toLowerCase();
	const normalizedModelId = modelId.toLowerCase();
	return (
		normalizedProvider.includes("codex") || normalizedModelId.includes("codex")
	);
}

export function useModelSelection(
	sessionId: string | null,
	workspacePath: string | null,
	locale: "en" | "de" = "en",
): ModelSelectionState & ModelSelectionActions {
	const { selectedChatFromHistory } = useSelectedChat();
	const { runnerSessions } = useChatContext();

	// Derive model from the active runner session for this sessionId.
	// This is available before chat history merges and localStorage is populated.
	const runnerSessionModelRef = useMemo(() => {
		if (!sessionId) return null;
		const rs = runnerSessions.find((s) => s.session_id === sessionId);
		if (rs?.provider && rs?.model) {
			return `${rs.provider}/${rs.model}`;
		}
		return null;
	}, [sessionId, runnerSessions]);

	const _normalizedWorkspacePath = useMemo(
		() => normalizeWorkspacePath(workspacePath),
		[workspacePath],
	);

	// Load cached models from localStorage instantly (no loading spinner)
	const modelCacheKey = useMemo(
		() =>
			_normalizedWorkspacePath
				? `oqto:modelCache:${_normalizedWorkspacePath}`
				: null,
		[_normalizedWorkspacePath],
	);

	const [availableModels, setAvailableModels] = useState<PiModelInfo[]>(() => {
		if (!modelCacheKey) return [];
		try {
			const cached = localStorage.getItem(modelCacheKey);
			if (cached) return JSON.parse(cached) as PiModelInfo[];
		} catch {
			// ignore
		}
		return [];
	});
	const [selectedModelRef, setSelectedModelRef] = useState<string | null>(null);
	const [pendingModelRef, setPendingModelRef] = useState<string | null>(null);
	const [modelsVersion, setModelsVersion] = useState(0);
	const [isSwitching, setIsSwitching] = useState(false);
	// Only show loading if we have no cached models
	const [loading, setLoading] = useState(false);
	const [isIdle, setIsIdle] = useState(true);

	// Refs to avoid stale closures in event handlers
	const pendingModelRefRef = useRef(pendingModelRef);
	pendingModelRefRef.current = pendingModelRef;
	const selectedModelRefRef = useRef(selectedModelRef);
	selectedModelRefRef.current = selectedModelRef;
	const lastReasoningDefaultKeyRef = useRef<string | null>(null);

	const effectiveSessionId = sessionId;
	const hookInstanceIdRef = useRef(
		`modelsel-${Math.random().toString(36).slice(2)}`,
	);

	const syncModelSelection = useCallback(
		(next: {
			selectedModelRef?: string | null;
			pendingModelRef?: string | null;
			isSwitching?: boolean;
		}) => {
			emitModelSelectionSync({
				sessionId: effectiveSessionId,
				source: hookInstanceIdRef.current,
				...next,
			});
		},
		[effectiveSessionId],
	);

	const setSelectedModelSynced = useCallback(
		(value: string | null) => {
			setSelectedModelRef(value);
			syncModelSelection({ selectedModelRef: value });
		},
		[syncModelSelection],
	);
	const setPendingModelSynced = useCallback(
		(value: string | null) => {
			setPendingModelRef(value);
			syncModelSelection({ pendingModelRef: value });
		},
		[syncModelSelection],
	);
	const setSwitchingSynced = useCallback(
		(value: boolean) => {
			setIsSwitching(value);
			syncModelSelection({ isSwitching: value });
		},
		[syncModelSelection],
	);

	// Storage key for persisting selected model
	const modelStorageKey = useMemo(() => {
		if (!effectiveSessionId) return null;
		return `oqto:chatModel:${effectiveSessionId}`;
	}, [effectiveSessionId]);
	const aliasModelStorageKey = useMemo(() => {
		if (!effectiveSessionId) return null;
		const alias = getRunnerHistoryAlias(effectiveSessionId);
		if (!alias || alias === effectiveSessionId) return null;
		return `oqto:chatModel:${alias}`;
	}, [effectiveSessionId]);

	const workspaceModelStorageKey = useMemo(
		() => getWorkspaceModelStorageKey(_normalizedWorkspacePath),
		[_normalizedWorkspacePath],
	);

	// Listen for OAuth model invalidation events and force a re-fetch.
	// useeffect-guardrail: allow - global window event subscription
	useEffect(() => {
		if (typeof window === "undefined") return undefined;
		const handler = () => {
			// Clear localStorage cache so the next fetch gets fresh models
			if (modelCacheKey) {
				try {
					localStorage.removeItem(modelCacheKey);
				} catch {
					// ignore
				}
			}
			setModelsVersion((v) => v + 1);
		};
		window.addEventListener(
			OAUTH_MODELS_INVALIDATE_EVENT,
			handler as EventListener,
		);
		return () => {
			window.removeEventListener(
				OAUTH_MODELS_INVALIDATE_EVENT,
				handler as EventListener,
			);
		};
	}, [modelCacheKey]);

	// Keep multiple model selectors (status bar + settings sidebar) in lockstep.
	// useeffect-guardrail: allow - window custom event subscription
	useEffect(() => {
		if (typeof window === "undefined") return undefined;
		const handler = (event: Event) => {
			const detail = (event as CustomEvent<ModelSelectionSyncDetail>).detail;
			if (!detail) return;
			if (detail.source === hookInstanceIdRef.current) return;
			if (detail.sessionId !== effectiveSessionId) return;
			if (detail.selectedModelRef !== undefined) {
				setSelectedModelRef(detail.selectedModelRef);
			}
			if (detail.pendingModelRef !== undefined) {
				setPendingModelRef(detail.pendingModelRef);
			}
			if (detail.isSwitching !== undefined) {
				setIsSwitching(detail.isSwitching);
			}
		};
		window.addEventListener(
			MODEL_SELECTION_SYNC_EVENT,
			handler as EventListener,
		);
		return () => {
			window.removeEventListener(
				MODEL_SELECTION_SYNC_EVENT,
				handler as EventListener,
			);
		};
	}, [effectiveSessionId]);

	// Load selected model: session storage > hstry session > workspace default > keep current
	useEffect(() => {
		const readStoredRef = (key: string | null) => {
			if (!key) return null;
			try {
				const stored = localStorage.getItem(key);
				return stored || null;
			} catch {
				return null;
			}
		};

		const sessionStored =
			readStoredRef(modelStorageKey) ?? readStoredRef(aliasModelStorageKey);
		if (sessionStored) {
			setSelectedModelSynced(sessionStored);
			try {
				if (modelStorageKey) {
					localStorage.setItem(modelStorageKey, sessionStored);
				}
				if (aliasModelStorageKey) {
					localStorage.setItem(aliasModelStorageKey, sessionStored);
				}
			} catch {
				// ignore localStorage errors
			}
			return;
		}

		// Fall back to model/provider from hstry ChatSession and immediately
		// persist under this concrete session key so reloads stay stable.
		if (selectedChatFromHistory?.provider && selectedChatFromHistory?.model) {
			const ref = `${selectedChatFromHistory.provider}/${selectedChatFromHistory.model}`;
			setSelectedModelSynced(ref);
			try {
				if (modelStorageKey) {
					localStorage.setItem(modelStorageKey, ref);
				}
				if (aliasModelStorageKey) {
					localStorage.setItem(aliasModelStorageKey, ref);
				}
			} catch {
				// ignore localStorage errors
			}
			return;
		}

		// Fall back to model reported by the active runner session.
		// This is available before chat history merges and before
		// config.model_changed events arrive.
		if (runnerSessionModelRef) {
			setSelectedModelSynced(runnerSessionModelRef);
			try {
				if (modelStorageKey) {
					localStorage.setItem(modelStorageKey, runnerSessionModelRef);
				}
				if (aliasModelStorageKey) {
					localStorage.setItem(aliasModelStorageKey, runnerSessionModelRef);
				}
			} catch {
				// ignore localStorage errors
			}
			return;
		}

		// Use workspace-wide default as fallback.
		const workspaceStored = readStoredRef(workspaceModelStorageKey);
		if (workspaceStored) {
			setSelectedModelSynced(workspaceStored);
			return;
		}

		// When we have an active session, the model info may still be loading
		// (runner session merge or config.model_changed event pending).
		// Preserve the current selection instead of resetting to null which
		// would flash "Select model" in the UI.
		if (!effectiveSessionId) {
			setSelectedModelSynced(null);
		}
	}, [
		modelStorageKey,
		workspaceModelStorageKey,
		effectiveSessionId,
		selectedChatFromHistory?.provider,
		selectedChatFromHistory?.model,
		runnerSessionModelRef,
		setSelectedModelSynced,
		aliasModelStorageKey,
	]);

	// Load available models - fetches once per session, retries if Pi not ready yet
	// biome-ignore lint/correctness/useExhaustiveDependencies: availableModels.length is stable
	useEffect(() => {
		let active = true;
		let retryTimer: ReturnType<typeof setTimeout> | null = null;

		const workdir = _normalizedWorkspacePath;
		const hasSession = Boolean(effectiveSessionId);
		const targetSessionId = effectiveSessionId ?? "_system";

		if (!hasSession && !workdir) {
			setAvailableModels([]);
			setLoading(false);
			return undefined;
		}

		const fetchModels = (attempt: number) => {
			if (!active) return;
			// Only show loading spinner if we have no cached models yet
			if (attempt === 0 && availableModels.length === 0) setLoading(true);

			getWsManager()
				.agentGetAvailableModels(targetSessionId, workdir ?? undefined)
				.then((result) => {
					if (!active) return;
					const models = (result as PiModelInfo[]) ?? [];
					if (models.length === 0 && attempt < 5) {
						// Pi process may still be initializing (live session) or
						// runner is spawning an ephemeral Pi to fetch models - retry with backoff
						retryTimer = setTimeout(
							() => fetchModels(attempt + 1),
							1000 * (attempt + 1),
						);
						return;
					}
					setAvailableModels(models);
					setLoading(false);
					// Cache to localStorage for instant display on next load
					if (models.length > 0 && modelCacheKey) {
						try {
							localStorage.setItem(modelCacheKey, JSON.stringify(models));
						} catch {
							// ignore quota errors
						}
					}
					if (models.length > 0 && !effectiveSessionId) {
						setSelectedModelRef((prev) => {
							if (prev) return prev;
							const first = models[0];
							const next = `${first.provider}/${first.id}`;
							syncModelSelection({ selectedModelRef: next });
							return next;
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
	}, [
		effectiveSessionId,
		_normalizedWorkspacePath,
		modelCacheKey,
		modelsVersion,
	]);

	// Derive idle/streaming state from agent events and fetch initial state.
	// Also handles applying pending model when agent goes idle.
	// useeffect-guardrail: allow - websocket subscription + initial idle state fetch
	useEffect(() => {
		if (!effectiveSessionId) {
			setIsIdle(true);
			return undefined;
		}

		const syncModelFromState = (
			state: {
				model?: {
					provider?: string;
					id?: string;
				};
			} | null,
		) => {
			const provider = state?.model?.provider;
			const modelId = state?.model?.id;
			if (!provider || !modelId) return;
			const modelRef = `${provider}/${modelId}`;
			if (selectedModelRefRef.current !== modelRef) {
				setSelectedModelSynced(modelRef);
			}
			if (pendingModelRefRef.current === modelRef) {
				setPendingModelSynced(null);
			}
			try {
				if (modelStorageKey) {
					localStorage.setItem(modelStorageKey, modelRef);
					if (aliasModelStorageKey) {
						localStorage.setItem(aliasModelStorageKey, modelRef);
					}
				} else {
					localStorage.setItem(workspaceModelStorageKey, modelRef);
				}
			} catch {
				// ignore localStorage errors
			}
		};

		void getWsManager()
			.agentGetStateWait(effectiveSessionId)
			.then((state) => {
				const s = state as {
					isStreaming: boolean;
					isCompacting: boolean;
					model?: {
						provider?: string;
						id?: string;
					};
				} | null;
				if (s) {
					setIsIdle(!s.isStreaming && !s.isCompacting);
					syncModelFromState(s);
				}
			})
			.catch(() => {
				// Ignore - default to idle
			});

		const unsubscribe = getWsManager().subscribe("agent", (event) => {
			if (!("channel" in event) || event.channel !== "agent") return;
			if (event.session_id !== effectiveSessionId) return;

			const eventType = event.event as string | undefined;

			if (eventType === "session.created") {
				setIsIdle(true);
				const pending = pendingModelRefRef.current;
				if (pending && effectiveSessionId) {
					void applyPendingModel(pending, effectiveSessionId, workspacePath);
				}
			}

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
					setSelectedModelSynced(newRef);
					persistModelSelection(newRef);
					if (pendingModelRefRef.current === newRef) {
						setPendingModelSynced(null);
					}
				}
			}
		});
		return unsubscribe;
	}, [
		effectiveSessionId,
		workspacePath,
		setSelectedModelSynced,
		setPendingModelSynced,
		modelStorageKey,
		aliasModelStorageKey,
		workspaceModelStorageKey,
	]);

	const persistModelSelection = useCallback(
		(modelRef: string | null) => {
			try {
				if (modelRef) {
					// Always keep workspace default in sync so brand-new sessions
					// inherit the latest user selection.
					localStorage.setItem(workspaceModelStorageKey, modelRef);
					if (modelStorageKey) {
						// Session-scoped selection (strict isolation between chats)
						localStorage.setItem(modelStorageKey, modelRef);
						if (aliasModelStorageKey) {
							localStorage.setItem(aliasModelStorageKey, modelRef);
						}
					}
				} else if (modelStorageKey) {
					localStorage.removeItem(modelStorageKey);
					if (aliasModelStorageKey) {
						localStorage.removeItem(aliasModelStorageKey);
					}
				}
			} catch {
				// ignore localStorage errors
			}
		},
		[modelStorageKey, aliasModelStorageKey, workspaceModelStorageKey],
	);

	const ensureReasoningThinkingDefault = useCallback(
		async (sid: string, modelRef: string) => {
			if (!isReasoningModel(modelRef, availableModels)) return;

			const marker = `${sid}:${modelRef}`;
			if (lastReasoningDefaultKeyRef.current === marker) return;

			try {
				const state = (await getWsManager().agentGetStateWait(sid)) as {
					thinkingLevel?: string;
				} | null;
				const currentLevel = (state?.thinkingLevel ?? "").toLowerCase();
				if (!currentLevel || currentLevel === "off") {
					await getWsManager().agentSetThinkingLevel(
						sid,
						DEFAULT_REASONING_THINKING_LEVEL,
					);
				}
				lastReasoningDefaultKeyRef.current = marker;
			} catch (err) {
				console.warn("Failed to enforce reasoning thinking default:", err);
			}
		},
		[availableModels],
	);

	useEffect(() => {
		if (!effectiveSessionId || !selectedModelRef) return;
		void ensureReasoningThinkingDefault(effectiveSessionId, selectedModelRef);
	}, [effectiveSessionId, selectedModelRef, ensureReasoningThinkingDefault]);

	// Helper to apply a pending model switch
	const applyPendingModel = async (
		modelRef: string,
		sid: string,
		wp: string | null,
	) => {
		setPendingModelSynced(null);
		const separatorIndex = modelRef.indexOf("/");
		if (separatorIndex <= 0 || separatorIndex >= modelRef.length - 1) return;

		const provider = modelRef.slice(0, separatorIndex);
		const modelId = modelRef.slice(separatorIndex + 1);
		try {
			await getWsManager().agentSetModel(sid, provider, modelId);
			// Switch confirmed -- update state and persist
			setSelectedModelSynced(modelRef);
			await ensureReasoningThinkingDefault(sid, modelRef);
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
			persistModelSelection(modelRef);
		} catch (err) {
			console.error("Failed to apply pending model:", err);
			// Revert localStorage to the actual model if we had persisted
			// the pending one eagerly (e.g. before a page reload).
			// The config.model_changed event will correct selectedModelRef
			// if/when a successful switch happens later.
		}
	};

	const persistSelection = useCallback(
		async (provider: string, modelId: string, modelRef: string) => {
			persistModelSelection(modelRef);
			try {
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
				console.error("Failed to persist model selection:", err);
			}
		},
		[persistModelSelection, workspacePath],
	);

	const selectModel = useCallback(
		async (modelRef: string) => {
			if (!modelRef) return;

			const separatorIndex = modelRef.indexOf("/");
			if (separatorIndex <= 0 || separatorIndex === modelRef.length - 1) {
				return;
			}

			const provider = modelRef.slice(0, separatorIndex);
			const modelId = modelRef.slice(separatorIndex + 1);

			if (!effectiveSessionId) {
				setSelectedModelSynced(modelRef);
				await persistSelection(provider, modelId, modelRef);
				return;
			}

			const manager = getWsManager();
			if (!manager.isSessionReady(effectiveSessionId)) {
				// Session not ready yet -- queue the switch and persist
				// so the pending model survives page reloads.
				setPendingModelSynced(modelRef);
				await persistSelection(provider, modelId, modelRef);
				return;
			}

			if (isIdle) {
				// Show optimistic switch indicator
				const previousModelRef = selectedModelRef;
				setSelectedModelSynced(modelRef);
				setSwitchingSynced(true);
				try {
					await manager.agentSetModel(effectiveSessionId, provider, modelId);
					await ensureReasoningThinkingDefault(effectiveSessionId, modelRef);
					// Only persist after confirmed success
					await persistSelection(provider, modelId, modelRef);
				} catch (err) {
					console.error("Failed to switch model:", err);
					const message = err instanceof Error ? err.message : String(err);
					if (
						message.includes("SessionNotFound") ||
						message.includes("PiSessionNotFound") ||
						message.includes("Response channel closed")
					) {
						// Session gone -- queue for when it comes back and
						// persist so the pending model survives reloads.
						setPendingModelSynced(modelRef);
						await persistSelection(provider, modelId, modelRef);
					} else {
						// Model switch failed (e.g. model not found) -- revert
						setSelectedModelSynced(previousModelRef);
					}
				} finally {
					setSwitchingSynced(false);
				}
			} else {
				// Queue for after streaming completes and persist so the
				// pending model survives page reloads.
				setPendingModelSynced(modelRef);
				await persistSelection(provider, modelId, modelRef);
			}
		},
		[
			isIdle,
			effectiveSessionId,
			persistSelection,
			selectedModelRef,
			ensureReasoningThinkingDefault,
			setSelectedModelSynced,
			setPendingModelSynced,
			setSwitchingSynced,
		],
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
