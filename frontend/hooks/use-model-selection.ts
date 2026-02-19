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
	const [isSwitching, setIsSwitching] = useState(false);
	// Only show loading if we have no cached models
	const [loading, setLoading] = useState(false);
	const [isIdle, setIsIdle] = useState(true);

	// Refs to avoid stale closures in event handlers
	const pendingModelRefRef = useRef(pendingModelRef);
	pendingModelRefRef.current = pendingModelRef;

	const effectiveSessionId = sessionId;

	// Storage key for persisting selected model
	const modelStorageKey = useMemo(() => {
		if (!effectiveSessionId) return null;
		return `oqto:chatModel:${effectiveSessionId}`;
	}, [effectiveSessionId]);

	const workspaceModelStorageKey = useMemo(
		() => getWorkspaceModelStorageKey(_normalizedWorkspacePath),
		[_normalizedWorkspacePath],
	);

	// Load selected model: session storage > workspace storage > hstry session > null
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

		const sessionStored = readStoredRef(modelStorageKey);
		if (sessionStored) {
			setSelectedModelRef(sessionStored);
			return;
		}

		const workspaceStored = readStoredRef(workspaceModelStorageKey);
		if (workspaceStored) {
			setSelectedModelRef(workspaceStored);
			return;
		}

		// Fall back to model/provider from hstry ChatSession
		if (selectedChatFromHistory?.provider && selectedChatFromHistory?.model) {
			const ref = `${selectedChatFromHistory.provider}/${selectedChatFromHistory.model}`;
			setSelectedModelRef(ref);
			return;
		}

		setSelectedModelRef(null);
	}, [
		modelStorageKey,
		workspaceModelStorageKey,
		selectedChatFromHistory?.provider,
		selectedChatFromHistory?.model,
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
	}, [effectiveSessionId, _normalizedWorkspacePath, modelCacheKey]);

	// Derive idle/streaming state from agent events (no polling)
	// Also handles applying pending model when agent goes idle
	useEffect(() => {
		if (!effectiveSessionId) return undefined;

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

	const persistModelSelection = useCallback(
		(modelRef: string | null) => {
			try {
				if (modelRef) {
					if (modelStorageKey) {
						localStorage.setItem(modelStorageKey, modelRef);
					}
					localStorage.setItem(workspaceModelStorageKey, modelRef);
				} else {
					if (modelStorageKey) {
						localStorage.removeItem(modelStorageKey);
					}
					localStorage.removeItem(workspaceModelStorageKey);
				}
			} catch {
				// ignore localStorage errors
			}
		},
		[modelStorageKey, workspaceModelStorageKey],
	);

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
			// Switch confirmed -- update state and persist
			setSelectedModelRef(modelRef);
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
				setSelectedModelRef(modelRef);
				await persistSelection(provider, modelId, modelRef);
				return;
			}

			const manager = getWsManager();
			if (!manager.isSessionReady(effectiveSessionId)) {
				// Session not ready yet -- queue the switch and persist
				// so the pending model survives page reloads.
				setPendingModelRef(modelRef);
				await persistSelection(provider, modelId, modelRef);
				return;
			}

			if (isIdle) {
				// Show optimistic switch indicator
				const previousModelRef = selectedModelRef;
				setSelectedModelRef(modelRef);
				setIsSwitching(true);
				try {
					await manager.agentSetModel(effectiveSessionId, provider, modelId);
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
						setPendingModelRef(modelRef);
						await persistSelection(provider, modelId, modelRef);
					} else {
						// Model switch failed (e.g. model not found) -- revert
						setSelectedModelRef(previousModelRef);
					}
				} finally {
					setIsSwitching(false);
				}
			} else {
				// Queue for after streaming completes and persist so the
				// pending model survives page reloads.
				setPendingModelRef(modelRef);
				await persistSelection(provider, modelId, modelRef);
			}
		},
		[isIdle, effectiveSessionId, persistSelection, selectedModelRef],
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
