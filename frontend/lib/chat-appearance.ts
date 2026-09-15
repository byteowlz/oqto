"use client";

import { useCallback, useEffect, useState } from "react";

/**
 * How the conversation is set. These are reading preferences, not layout
 * state: they belong to the person, not to the session, so they are stored
 * host-wide next to the chat detail level.
 *
 * The values are written onto the document element as data attributes and
 * read back by the chat typography stylesheet, which owns every actual
 * measurement. Nothing here knows a pixel.
 */

export type ChatProseFont = "schibsted" | "atkinson" | "system" | "mono";
export type ChatMeasure = "full" | "narrow" | "normal" | "wide";
export type ChatTurnShape = "bubble" | "block" | "plain";
/** How a filename named mid-sentence is set. */
export type ChatFileRef = "bare" | "subtle";
/** Where the reading column sits in a pane wider than the measure. */
export type ChatColumn = "centre" | "left";
/** How an assistant turn is bounded: a rule above it, a box, or nothing. */
export type ChatAgentTurn = "ruled" | "boxed" | "plain";
/**
 * Whether the per-message actions wait to be hovered. "always" is what a
 * device without a pointer gets regardless, since there is no hover to wait
 * for there.
 */
export type ChatActionVisibility = "hover" | "always";

export interface ChatAppearance {
	font: ChatProseFont;
	measure: ChatMeasure;
	turn: ChatTurnShape;
	fileRef: ChatFileRef;
	actions: ChatActionVisibility;
	column: ChatColumn;
	agentTurn: ChatAgentTurn;
}

export const DEFAULT_CHAT_APPEARANCE: ChatAppearance = {
	font: "schibsted",
	measure: "full",
	turn: "bubble",
	fileRef: "bare",
	actions: "hover",
	column: "centre",
	agentTurn: "boxed",
};

const STORAGE_KEY = "oqto:chatAppearance";
const EVENT_NAME = "oqto:chat-appearance";

const FONTS: readonly ChatProseFont[] = [
	"schibsted",
	"atkinson",
	"system",
	"mono",
];
const MEASURES: readonly ChatMeasure[] = ["full", "narrow", "normal", "wide"];
const TURNS: readonly ChatTurnShape[] = ["bubble", "block", "plain"];
const FILE_REFS: readonly ChatFileRef[] = ["bare", "subtle"];
const ACTIONS: readonly ChatActionVisibility[] = ["hover", "always"];
const COLUMNS: readonly ChatColumn[] = ["centre", "left"];
const AGENT_TURNS: readonly ChatAgentTurn[] = ["boxed", "ruled", "plain"];

function pick<T extends string>(
	value: unknown,
	allowed: readonly T[],
	fallback: T,
): T {
	return typeof value === "string" &&
		(allowed as readonly string[]).includes(value)
		? (value as T)
		: fallback;
}

function coerce(raw: unknown): ChatAppearance {
	if (!raw || typeof raw !== "object") return DEFAULT_CHAT_APPEARANCE;
	const value = raw as Partial<Record<keyof ChatAppearance, unknown>>;
	return {
		font: pick(value.font, FONTS, DEFAULT_CHAT_APPEARANCE.font),
		measure: pick(value.measure, MEASURES, DEFAULT_CHAT_APPEARANCE.measure),
		turn: pick(value.turn, TURNS, DEFAULT_CHAT_APPEARANCE.turn),
		fileRef: pick(value.fileRef, FILE_REFS, DEFAULT_CHAT_APPEARANCE.fileRef),
		actions: pick(value.actions, ACTIONS, DEFAULT_CHAT_APPEARANCE.actions),
		column: pick(value.column, COLUMNS, DEFAULT_CHAT_APPEARANCE.column),
		agentTurn: pick(
			value.agentTurn,
			AGENT_TURNS,
			DEFAULT_CHAT_APPEARANCE.agentTurn,
		),
	};
}

export function readChatAppearance(): ChatAppearance {
	if (typeof window === "undefined") return DEFAULT_CHAT_APPEARANCE;
	try {
		const stored = window.localStorage.getItem(STORAGE_KEY);
		if (!stored) return DEFAULT_CHAT_APPEARANCE;
		return coerce(JSON.parse(stored));
	} catch {
		return DEFAULT_CHAT_APPEARANCE;
	}
}

/**
 * Publishes the appearance onto the document element. The stylesheet keys off
 * these attributes, so a change is one attribute write rather than a re-render
 * of the transcript.
 */
export function applyChatAppearance(appearance: ChatAppearance): void {
	if (typeof document === "undefined") return;
	const root = document.documentElement;
	root.dataset.chatFont = appearance.font;
	root.dataset.chatMeasure = appearance.measure;
	root.dataset.chatTurn = appearance.turn;
	root.dataset.chatFileref = appearance.fileRef;
	root.dataset.chatActions = appearance.actions;
	root.dataset.chatColumn = appearance.column;
	root.dataset.chatAgent = appearance.agentTurn;
}

export function useChatAppearance() {
	const [appearance, setAppearanceState] = useState<ChatAppearance>(() =>
		readChatAppearance(),
	);

	const setAppearance = useCallback((next: Partial<ChatAppearance>) => {
		const merged = coerce({ ...readChatAppearance(), ...next });
		setAppearanceState(merged);
		applyChatAppearance(merged);
		try {
			window.localStorage.setItem(STORAGE_KEY, JSON.stringify(merged));
		} catch {
			// A browser with storage disabled still gets the change for this
			// session; only persistence is lost.
		}
		window.dispatchEvent(new CustomEvent(EVENT_NAME, { detail: merged }));
	}, []);

	useEffect(() => {
		applyChatAppearance(appearance);
		const handleStorage = (event: StorageEvent) => {
			if (event.key !== STORAGE_KEY) return;
			const next = readChatAppearance();
			setAppearanceState(next);
			applyChatAppearance(next);
		};
		const handleCustom = (event: Event) => {
			const next = coerce((event as CustomEvent<unknown>).detail);
			setAppearanceState(next);
			applyChatAppearance(next);
		};
		window.addEventListener("storage", handleStorage);
		window.addEventListener(EVENT_NAME, handleCustom);
		return () => {
			window.removeEventListener("storage", handleStorage);
			window.removeEventListener(EVENT_NAME, handleCustom);
		};
	}, [appearance]);

	return { appearance, setAppearance };
}
