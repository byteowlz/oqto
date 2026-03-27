export type SessionIdentityState =
	| { kind: "unbound"; clientId: string }
	| { kind: "bound"; runnerId: string; hstryId?: string; piId?: string };

export type TurnState =
	| { kind: "idle" }
	| { kind: "sending"; commandId?: string; clientMessageId?: string }
	| { kind: "streaming"; turnId?: string }
	| { kind: "reconciling"; reason: "idle" | "resync" | "reconnect" }
	| { kind: "error"; recoverable: boolean; message: string };

export type TransportState =
	| { kind: "disconnected"; epoch: number }
	| { kind: "connecting"; epoch: number }
	| { kind: "connected"; epoch: number }
	| { kind: "reconnecting"; epoch: number };

export type MessageSyncSource =
	| "history"
	| "resync"
	| "ws_get_messages"
	| "ws_messages"
	| "watchdog";

export type MessageMergeMode = "authoritative" | "partial";

export interface MessageSyncState {
	phase: "idle" | "syncing";
	lastSource?: MessageSyncSource;
	revision: number;
}

export interface ChatStateMachine {
	identity: SessionIdentityState;
	turn: TurnState;
	transport: TransportState;
	sync: MessageSyncState;
}

export function createInitialChatStateMachine(
	clientId: string | null,
): ChatStateMachine {
	return {
		identity: {
			kind: "unbound",
			clientId: clientId ?? "unbound-session",
		},
		turn: { kind: "idle" },
		transport: { kind: "disconnected", epoch: 0 },
		sync: { phase: "idle", revision: 0 },
	};
}

export function bindIdentity(
	machine: ChatStateMachine,
	payload: { runnerId: string; hstryId?: string; piId?: string },
): ChatStateMachine {
	if (!payload.runnerId.trim()) return machine;
	if (machine.identity.kind === "bound") {
		if (machine.identity.runnerId === payload.runnerId) {
			return {
				...machine,
				identity: {
					kind: "bound",
					runnerId: payload.runnerId,
					hstryId: payload.hstryId ?? machine.identity.hstryId,
					piId: payload.piId ?? machine.identity.piId,
				},
			};
		}
		// Rebinding to a different runner ID is only valid when idle.
		if (machine.turn.kind !== "idle") {
			return machine;
		}
	}
	return {
		...machine,
		identity: {
			kind: "bound",
			runnerId: payload.runnerId,
			hstryId: payload.hstryId,
			piId: payload.piId,
		},
	};
}

export function resetIdentity(
	machine: ChatStateMachine,
	clientId: string,
): ChatStateMachine {
	return {
		...machine,
		identity: { kind: "unbound", clientId },
		turn: { kind: "idle" },
	};
}

export function transitionTurn(
	machine: ChatStateMachine,
	next: TurnState,
): ChatStateMachine {
	// Enforce: non-idle turn states require bound identity.
	if (next.kind !== "idle" && machine.identity.kind !== "bound") {
		return machine;
	}

	const from = machine.turn.kind;
	const to = next.kind;

	const allowed: Record<TurnState["kind"], TurnState["kind"][]> = {
		idle: ["sending", "reconciling", "error", "idle"],
		sending: ["streaming", "reconciling", "error", "idle"],
		streaming: ["reconciling", "error", "idle", "streaming"],
		reconciling: ["idle", "streaming", "error", "reconciling"],
		error: ["idle", "reconciling", "sending", "error"],
	};

	if (!allowed[from].includes(to)) {
		return machine;
	}

	return { ...machine, turn: next };
}

export function transitionTransport(
	machine: ChatStateMachine,
	nextKind: TransportState["kind"],
	epoch: number,
): ChatStateMachine {
	if (epoch < machine.transport.epoch) return machine;
	return {
		...machine,
		transport: {
			kind: nextKind,
			epoch,
		},
	};
}

export function deriveUiFlags(turn: TurnState): {
	isStreaming: boolean;
	isAwaitingResponse: boolean;
} {
	return {
		isStreaming: turn.kind === "streaming",
		isAwaitingResponse: turn.kind === "sending",
	};
}

export function beginMessageSync(
	machine: ChatStateMachine,
	source: MessageSyncSource,
): ChatStateMachine {
	return {
		...machine,
		sync: {
			...machine.sync,
			phase: "syncing",
			lastSource: source,
		},
	};
}

export function completeMessageSync(
	machine: ChatStateMachine,
	source: MessageSyncSource,
): ChatStateMachine {
	return {
		...machine,
		sync: {
			phase: "idle",
			lastSource: source,
			revision: machine.sync.revision + 1,
		},
	};
}

export function selectMessageMergeMode(
	machine: ChatStateMachine,
	source: MessageSyncSource,
): MessageMergeMode {
	// During active streaming, ANY source may return stale data from a
	// previous turn. Use partial merge to preserve in-flight optimistic
	// messages and streaming state. This prevents a race where a previous
	// turn's agent.idle triggers a history fetch, but a new turn starts
	// before the fetch completes — the stale authoritative merge would
	// wipe the new turn's messages.
	if (machine.turn.kind === "streaming") {
		return "partial";
	}
	return "authoritative";
}
