/**
 * Turn machine: session identity, turn lifecycle, transport state.
 *
 * Pure transitions ported from the legacy chat state machine. Identity is
 * the platform session id; drafts and pages are keyed by it. Message sync
 * phases are gone: the timeline page cache owns durable state.
 */

export type SessionIdentityState =
	| { kind: "unbound"; clientId: string }
	| { kind: "bound"; runnerId: string; hstryId?: string; piId?: string };

export type TurnState =
	| { kind: "idle" }
	| { kind: "sending"; commandId?: string; clientMessageId?: string }
	| { kind: "streaming"; turnId?: string }
	| { kind: "syncing" }
	| { kind: "error"; recoverable: boolean; message: string };

export type TransportState =
	| { kind: "disconnected"; epoch: number }
	| { kind: "connecting"; epoch: number }
	| { kind: "connected"; epoch: number }
	| { kind: "reconnecting"; epoch: number };

export interface ChatStateMachine {
	identity: SessionIdentityState;
	turn: TurnState;
	transport: TransportState;
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
	};
}

export type IdentityBinding = {
	runnerId: string;
	hstryId?: string;
	piId?: string;
};

export function bindIdentity(
	machine: ChatStateMachine,
	payload: IdentityBinding,
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
		// Allow idle -> streaming for runner-initiated turns where we receive
		// stream.message_start without a local send() transition first.
		idle: ["sending", "streaming", "syncing", "error", "idle"],
		sending: ["streaming", "syncing", "error", "idle"],
		streaming: ["syncing", "error", "idle", "streaming"],
		syncing: ["idle", "streaming", "error", "syncing"],
		error: ["idle", "syncing", "sending", "error"],
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
