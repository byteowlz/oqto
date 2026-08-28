import { useIsMobile } from "@/hooks/use-mobile";
import { useMountEffect } from "@/hooks/use-mount-effect";
import type { DisplayMessage, DisplayPart } from "@/lib/chat-render-types";
import {
	type ChatFileAdapter,
	MessageGroupCard,
} from "@/lib/chat-rendering/CanonicalMessageRenderer";
import {
	type MessageGroup as CanonicalMessageGroup,
	groupMessages,
} from "@/lib/chat-rendering/group-messages";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useVirtualizer } from "@tanstack/react-virtual";
import { CircleStop, Paperclip, Send } from "lucide-react";
import { useCallback, useLayoutEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import type { ChatTurnDraft } from "../platform/chat-contract";
import type {
	ChatMessage,
	OqtoUiPlatform,
	SessionTask,
} from "../platform/contracts";
import { workspaceFilePreviewUrl } from "../platform/workspace-file-url";
import { TaskProgress } from "./TaskProgress";
import { timelineQueryKey, turnDraftQueryKey } from "./query-keys";
import { useTimeline } from "./useTimeline";

type ChatPaneProps = {
	platform: Pick<OqtoUiPlatform, "id" | "loadMessages" | "chat">;
	agentName: string;
	sessionId: string;
	tasks: SessionTask[];
	workspacePath: string;
	onOpenFile: (
		path: string,
		range?: { startLine?: number; endLine?: number },
	) => void;
};

const OQTO_UI_CHAT_FILE_ADAPTER: ChatFileAdapter = {
	fileUrl: workspaceFilePreviewUrl,
};

type PendingPrompt = { id: string; text: string };

type DurableVisualGroup = {
	id: string;
	group: CanonicalMessageGroup;
};

function timestampFromDisplayTime(time: string, fallback: number): number {
	const match = time.match(/^(\d{1,2}):(\d{2})$/);
	if (!match) return fallback;
	const timestamp = new Date();
	timestamp.setHours(Number(match[1]), Number(match[2]), 0, 0);
	return timestamp.getTime();
}

function groupDurableMessages(messages: ChatMessage[]): DurableVisualGroup[] {
	const displayMessages: DisplayMessage[] = messages.map((message, index) => ({
		id: message.id,
		role:
			message.author === "agent"
				? "assistant"
				: message.author === "tool"
					? "tool"
					: "user",
		parts: (message.parts ?? [
			{ type: "text", id: `${message.id}:text`, text: message.content },
		]) as DisplayPart[],
		timestamp: timestampFromDisplayTime(message.time, index),
	}));

	return groupMessages(displayMessages).map((group) => ({
		id: group.messages.map((message) => message.id).join(":"),
		group,
	}));
}

function pendingPromptGroup(prompt: PendingPrompt): CanonicalMessageGroup {
	return {
		role: "user",
		messages: [
			{
				id: prompt.id,
				role: "user",
				parts: [{ type: "text", id: `${prompt.id}:text`, text: prompt.text }],
				timestamp: 0,
			},
		],
	};
}

function streamingDraftGroup(draft: ChatTurnDraft): CanonicalMessageGroup {
	const parts: DisplayPart[] = draft.parts.flatMap((part): DisplayPart[] => {
		if (part.type !== "tool_call") return [part as DisplayPart];
		const toolCall: DisplayPart = {
			type: "tool_call",
			id: part.id,
			toolCallId: part.toolCallId,
			name: part.name,
			input: part.input,
			status: part.status,
		};
		if (part.output === undefined) return [toolCall];
		return [
			toolCall,
			{
				type: "tool_result",
				id: `${part.id}:result`,
				toolCallId: part.toolCallId,
				name: part.name,
				output: part.output,
				isError: part.status === "error",
			},
		];
	});
	return {
		role: "assistant",
		messages: [
			{
				id: "streaming-draft",
				role: "assistant",
				parts,
				timestamp: 0,
				isStreaming: true,
			},
		],
	};
}

/**
 * The writable timeline: durable pages from the store, the ephemeral turn
 * draft and pending prompt from the engine, and a live composer. Store
 * truth always wins: turn end and resync drop ephemera and invalidate.
 */
export function ChatPane({
	platform,
	agentName,
	sessionId,
	tasks,
	workspacePath,
	onOpenFile,
}: ChatPaneProps) {
	const { t, i18n } = useTranslation();
	const locale = i18n.resolvedLanguage?.startsWith("de") ? "de" : "en";
	const queryClient = useQueryClient();
	const compact = useIsMobile(1024);
	const timeline = useTimeline(platform, sessionId);
	const draftKey = turnDraftQueryKey(platform.id, sessionId);
	const draftQuery = useQuery<ChatTurnDraft | null>({
		queryKey: draftKey,
		queryFn: () => Promise.resolve(null),
		initialData: null,
		enabled: false,
		staleTime: Number.POSITIVE_INFINITY,
	});
	const draft = draftQuery.data;
	const durableGroups = useMemo(
		() => groupDurableMessages(timeline.messages),
		[timeline.messages],
	);
	const [pendingPrompt, setPendingPrompt] = useState<PendingPrompt | null>(
		null,
	);
	const [prompt, setPrompt] = useState("");
	const [streaming, setStreaming] = useState(false);

	useMountEffect(() => {
		return platform.chat.bind(sessionId, (update) => {
			if (update.kind === "draft") {
				queryClient.setQueryData(draftKey, update.draft);
				setStreaming(true);
				return;
			}
			if (update.kind === "connection") return;
			// Turn ended or resync: disposable draft out, durable pages in.
			queryClient.setQueryData(draftKey, null);
			setStreaming(false);
			setPendingPrompt(null);
			queryClient.invalidateQueries({
				queryKey: timelineQueryKey(platform.id, sessionId),
			});
		});
	});

	const sendPrompt = (text: string) => {
		const clientId = platform.chat.send(sessionId, text, "steer");
		setPendingPrompt({ id: clientId, text });
		setStreaming(true);
		setPrompt("");
	};

	const scrollRef = useRef<HTMLElement | null>(null);
	// Viewport anchoring: keep the visible content stable when an earlier page
	// prepends, and follow the tail until the user scrolls away from it.
	const anchorRef = useRef({
		totalSize: 0,
		sessionId,
		firstMessageId: null as string | null,
	});
	const followTailRef = useRef(true);

	const getItemKey = useCallback(
		(index: number) => durableGroups[index]?.id ?? index,
		[durableGroups],
	);

	const virtualizer = useVirtualizer({
		count: compact ? 0 : durableGroups.length,
		getScrollElement: () => scrollRef.current,
		// Close to the real average row height; large misestimates make the
		// scrollbar and viewport visibly jump when rows measure.
		estimateSize: () => 76,
		overscan: 5,
		getItemKey,
		isScrollingResetDelay: 150,
	});

	const firstMessageId = durableGroups[0]?.id ?? null;
	useLayoutEffect(() => {
		const element = scrollRef.current;
		if (!element) return;
		const anchor = anchorRef.current;
		if (anchor.sessionId !== sessionId) {
			anchorRef.current = {
				totalSize: 0,
				sessionId,
				firstMessageId: null,
			};
			followTailRef.current = true;
			return;
		}
		const totalSize = compact
			? element.scrollHeight
			: virtualizer.getTotalSize();
		const grew = totalSize - anchor.totalSize;
		// Only touch scrollTop when the content actually changed: writing it
		// on every commit fights the user's momentum while scrolling.
		if (grew === 0) return;
		// Mere size drift (mobile URL-bar collapse, keyboard, viewport resize
		// re-measures) must NOT shift the reading position; only a real
		// prepended page may move the offset by its growth.
		const prepended =
			firstMessageId !== null && anchor.firstMessageId !== firstMessageId;
		anchorRef.current = { totalSize, sessionId, firstMessageId };
		if (followTailRef.current) {
			element.scrollTop = element.scrollHeight;
		} else if (prepended && grew > 0) {
			element.scrollTop += grew;
		}
	});

	const visibleItems = virtualizer.getVirtualItems();
	const autoLoadRef = useRef({ hasMore: false, loading: false });
	autoLoadRef.current = {
		hasMore: timeline.hasMore,
		loading: timeline.loadingEarlier,
	};

	return (
		<>
			<section
				className="wb-chat-panel"
				aria-label={t("oqtoUi.chat.timeline")}
				ref={scrollRef}
				onScroll={(event) => {
					const element = event.currentTarget;
					const distanceToBottom =
						element.scrollHeight - element.scrollTop - element.clientHeight;
					followTailRef.current = distanceToBottom < 48;
					// Reaching the top loads the next-older page.
					if (
						element.scrollTop < 64 &&
						autoLoadRef.current.hasMore &&
						!autoLoadRef.current.loading
					) {
						timeline.loadEarlier();
					}
				}}
			>
				{timeline.hasMore || timeline.loadingEarlier ? (
					<button
						className="wb-load-earlier"
						type="button"
						onClick={timeline.loadEarlier}
						disabled={timeline.loadingEarlier}
					>
						{timeline.loadingEarlier
							? t("oqtoUi.chat.loadingEarlier")
							: t("oqtoUi.chat.loadEarlier")}
					</button>
				) : null}
				<div
					className={compact ? "wb-timeline wb-timeline--plain" : "wb-timeline"}
					style={
						{
							// Empty invalidates `height` back to auto in plain flow.
							"--timeline-size": compact
								? ""
								: `${virtualizer.getTotalSize()}px`,
						} as React.CSSProperties
					}
				>
					{compact
						? durableGroups.map((group) => (
								<div className="wb-timeline-row" key={group.id}>
									<MessageGroupCard
										group={group.group}
										assistantName={agentName}
										workspacePath={workspacePath}
										fileAdapter={OQTO_UI_CHAT_FILE_ADAPTER}
										locale={locale}
										messageId={group.group.messages[0]?.id}
										onFileReferenceOpen={(path, range) =>
											onOpenFile(path, range)
										}
									/>
								</div>
							))
						: visibleItems.map((row) => {
								const group = durableGroups[row.index];
								if (!group) return null;
								return (
									<div
										className="wb-timeline-row"
										key={row.key}
										data-index={row.index}
										ref={virtualizer.measureElement}
										style={
											{
												"--row-offset": `${row.start}px`,
											} as React.CSSProperties
										}
									>
										<MessageGroupCard
											group={group.group}
											assistantName={agentName}
											workspacePath={workspacePath}
											fileAdapter={OQTO_UI_CHAT_FILE_ADAPTER}
											locale={locale}
											messageId={group.group.messages[0]?.id}
											onFileReferenceOpen={(path, range) =>
												onOpenFile(path, range)
											}
										/>
									</div>
								);
							})}
					{pendingPrompt ? (
						<div className="wb-timeline-row" key={pendingPrompt.id}>
							<MessageGroupCard
								group={pendingPromptGroup(pendingPrompt)}
								assistantName={agentName}
								workspacePath={workspacePath}
								fileAdapter={OQTO_UI_CHAT_FILE_ADAPTER}
								locale={locale}
								messageId={pendingPrompt.id}
								onFileReferenceOpen={(path, range) => onOpenFile(path, range)}
							/>
						</div>
					) : null}
					{draft ? (
						<div className="wb-timeline-row" key="streaming-draft">
							<MessageGroupCard
								group={streamingDraftGroup(draft)}
								assistantName={agentName}
								workspacePath={workspacePath}
								fileAdapter={OQTO_UI_CHAT_FILE_ADAPTER}
								locale={locale}
								messageId="streaming-draft"
								showWorkingIndicator
								onFileReferenceOpen={(path, range) => onOpenFile(path, range)}
							/>
						</div>
					) : null}
				</div>
			</section>

			<TaskProgress tasks={tasks} placement="desktop" />

			<footer className="wb-composer">
				<button
					className="wb-icon-button"
					type="button"
					aria-label={t("oqtoUi.chat.attach")}
				>
					<Paperclip aria-hidden="true" />
				</button>
				<textarea
					rows={1}
					placeholder={t("oqtoUi.chat.placeholder")}
					aria-label={t("oqtoUi.chat.placeholder")}
					value={prompt}
					onChange={(event) => setPrompt(event.target.value)}
					onKeyDown={(event) => {
						if (event.key === "Enter" && !event.shiftKey) {
							event.preventDefault();
							if (prompt.trim()) sendPrompt(prompt.trim());
						}
					}}
				/>
				{streaming ? (
					<button
						className="wb-icon-button"
						type="button"
						aria-label={t("oqtoUi.chat.abort")}
						onClick={() => platform.chat.abort(sessionId)}
					>
						<CircleStop aria-hidden="true" />
					</button>
				) : (
					<button
						className="wb-icon-button"
						type="button"
						aria-label={t("oqtoUi.chat.send")}
						disabled={!prompt.trim()}
						onClick={() => {
							if (prompt.trim()) sendPrompt(prompt.trim());
						}}
					>
						<Send aria-hidden="true" />
					</button>
				)}
			</footer>
		</>
	);
}
