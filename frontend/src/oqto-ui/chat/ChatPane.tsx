import { groupMessages } from "@/features/chat/rendering/group-messages";
import { useIsMobile } from "@/hooks/use-mobile";
import { useMountEffect } from "@/hooks/use-mount-effect";
import type { DisplayMessage, DisplayPart } from "@/lib/chat-render-types";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useVirtualizer } from "@tanstack/react-virtual";
import {
	Bot,
	CircleStop,
	Copy,
	FileEdit,
	FileText,
	GitBranch,
	Paperclip,
	Send,
	TestTube2,
	User,
} from "lucide-react";
import { useCallback, useLayoutEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import type { ChatTurnDraft } from "../platform/chat-contract";
import type {
	ChatMessage,
	ChatMessagePart,
	OqtoUiPlatform,
	SessionTask,
} from "../platform/contracts";
import { MessageParts } from "./MessageParts";
import { TaskProgress } from "./TaskProgress";
import { timelineQueryKey, turnDraftQueryKey } from "./query-keys";
import { useTimeline } from "./useTimeline";

type ChatPaneProps = {
	platform: Pick<OqtoUiPlatform, "id" | "loadMessages" | "chat">;
	agentName: string;
	sessionId: string;
	tasks: SessionTask[];
	onOpenFile: (
		path: string,
		range?: { startLine?: number; endLine?: number },
	) => void;
};

type PendingPrompt = { id: string; text: string };

type ActivityKind = NonNullable<ChatMessage["activity"]>["kind"];

type ToolIconProps = { kind: ActivityKind };

function ToolIcon({ kind }: ToolIconProps) {
	if (kind === "edit") return <FileEdit aria-hidden="true" />;
	if (kind === "test") return <TestTube2 aria-hidden="true" />;
	return <FileText aria-hidden="true" />;
}

type ToolResultPart = Extract<ChatMessagePart, { type: "tool_result" }>;

type DurableVisualGroup = {
	id: string;
	message: ChatMessage;
};

function groupDurableMessages(messages: ChatMessage[]): DurableVisualGroup[] {
	const sourceById = new Map(messages.map((message) => [message.id, message]));
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
		timestamp: index,
	}));

	return groupMessages(displayMessages).map((group) => {
		const source = sourceById.get(group.messages[0]?.id ?? "");
		const parts = group.messages.flatMap((message) => message.parts);
		return {
			id: group.messages.map((message) => message.id).join(":"),
			message: {
				id: group.messages[0]?.id ?? "empty-group",
				author:
					group.role === "user"
						? "user"
						: group.role === "tool"
							? "tool"
							: "agent",
				content: parts
					.filter(
						(part): part is Extract<DisplayPart, { type: "text" }> =>
							part.type === "text",
					)
					.map((part) => part.text)
					.join("\n\n"),
				time: source?.time ?? "",
				parts: parts as ChatMessagePart[],
			},
		};
	});
}

type MessageGroupProps = {
	message: ChatMessage;
	agentName: string;
	sessionId: string;
	resultByCallId: ReadonlyMap<string, ToolResultPart>;
	knownCallIds: ReadonlySet<string>;
	onOpenFile: ChatPaneProps["onOpenFile"];
	pending?: boolean;
};

function MessageGroup({
	message,
	agentName,
	sessionId,
	resultByCallId,
	knownCallIds,
	onOpenFile,
	pending,
}: MessageGroupProps) {
	const { t } = useTranslation();
	const isUser = message.author === "user";
	const name = isUser
		? t("oqtoUi.person.name")
		: message.author === "tool"
			? t("oqtoUi.chat.tool")
			: agentName;
	return (
		<article
			className="wb-msg"
			data-author={message.author}
			data-pending={pending}
		>
			<header className="wb-msg__header">
				{isUser ? <User aria-hidden="true" /> : <Bot aria-hidden="true" />}
				<span className="wb-msg__name">{name}</span>
				<span className="wb-msg__spacer" />
				{isUser ? (
					<button
						className="wb-icon-button"
						type="button"
						aria-label={t("oqtoUi.chat.forkHere")}
					>
						<GitBranch aria-hidden="true" />
					</button>
				) : null}
				<span className="wb-msg__time">{message.time}</span>
				<button
					className="wb-icon-button"
					type="button"
					aria-label={t("oqtoUi.chat.copy")}
				>
					<Copy aria-hidden="true" />
				</button>
			</header>
			<div className="wb-msg__body">
				<MessageParts
					message={message}
					sessionId={sessionId}
					resultByCallId={resultByCallId}
					knownCallIds={knownCallIds}
					onOpenFile={onOpenFile}
				/>
				{message.activity ? (
					<div className="wb-tool" data-kind={message.activity.kind}>
						<ToolIcon kind={message.activity.kind} />
						<span className="wb-tool__label">
							{t(`oqtoUi.activity.${message.activity.kind}`)}
						</span>
						<span className="wb-tool__target">{message.activity.name}</span>
					</div>
				) : null}
			</div>
		</article>
	);
}

type DraftPartViewProps = {
	part: ChatTurnDraft["parts"][number];
	sessionId: string;
	onOpenFile: ChatPaneProps["onOpenFile"];
};

function DraftPartView({ part, sessionId, onOpenFile }: DraftPartViewProps) {
	const { t } = useTranslation();
	if (part.type === "text") {
		return (
			<article className="wb-msg" data-author="agent" data-streaming="true">
				<header className="wb-msg__header">
					<Bot aria-hidden="true" />
					<span className="wb-msg__name">{t("oqtoUi.chat.stream")}</span>
					<span className="wb-msg__spacer" />
				</header>
				<div className="wb-msg__body">
					<MessageParts
						message={{
							id: `draft:${part.id}`,
							author: "agent",
							content: part.text,
							time: "",
							parts: [{ type: "text", id: part.id, text: part.text }],
						}}
						sessionId={sessionId}
						resultByCallId={new Map()}
						knownCallIds={new Set()}
						onOpenFile={onOpenFile}
					/>
				</div>
			</article>
		);
	}
	if (part.type === "thinking") {
		return (
			<div className="wb-draft-note">
				<Bot aria-hidden="true" />
				{part.text}
			</div>
		);
	}
	if (part.type === "tool_call") {
		const result: ToolResultPart | undefined =
			part.output === undefined
				? undefined
				: {
						type: "tool_result",
						id: `${part.id}:result`,
						toolCallId: part.toolCallId,
						name: part.name,
						output: part.output,
						isError: part.status === "error",
					};
		return (
			<MessageParts
				message={{
					id: `draft:${part.id}`,
					author: "agent",
					content: "",
					time: "",
					parts: [
						{
							type: "tool_call",
							id: part.id,
							toolCallId: part.toolCallId,
							name: part.name,
							input: part.input,
							status: part.status,
						},
					],
				}}
				sessionId={sessionId}
				resultByCallId={
					result ? new Map([[part.toolCallId, result]]) : new Map()
				}
				knownCallIds={new Set([part.toolCallId])}
				onOpenFile={onOpenFile}
			/>
		);
	}
	if (part.type === "compaction") {
		return <div className="wb-draft-note">{part.text}</div>;
	}
	return (
		<div className="wb-draft-note" data-retrying={part.retrying}>
			{part.text}
		</div>
	);
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
	onOpenFile,
}: ChatPaneProps) {
	const { t } = useTranslation();
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
	const { resultByCallId, knownCallIds } = useMemo(() => {
		const results = new Map<string, ToolResultPart>();
		const calls = new Set<string>();
		for (const message of timeline.messages) {
			for (const part of message.parts ?? []) {
				if (part.type === "tool_call") calls.add(part.toolCallId);
				if (part.type === "tool_result") results.set(part.toolCallId, part);
			}
		}
		return { resultByCallId: results, knownCallIds: calls };
	}, [timeline.messages]);
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
									<MessageGroup
										agentName={agentName}
										message={group.message}
										sessionId={sessionId}
										resultByCallId={resultByCallId}
										knownCallIds={knownCallIds}
										onOpenFile={onOpenFile}
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
										<MessageGroup
											agentName={agentName}
											message={group.message}
											sessionId={sessionId}
											resultByCallId={resultByCallId}
											knownCallIds={knownCallIds}
											onOpenFile={onOpenFile}
										/>
									</div>
								);
							})}
					{pendingPrompt ? (
						<div className="wb-timeline-row" key={pendingPrompt.id}>
							<MessageGroup
								agentName={agentName}
								message={{
									id: pendingPrompt.id,
									author: "user",
									content: pendingPrompt.text,
									time: t("oqtoUi.chat.pending"),
								}}
								sessionId={sessionId}
								resultByCallId={resultByCallId}
								knownCallIds={knownCallIds}
								onOpenFile={onOpenFile}
								pending
							/>
						</div>
					) : null}
					{draft
						? draft.parts.map((part) => (
								<div className="wb-timeline-row" key={part.id}>
									<DraftPartView
										part={part}
										sessionId={sessionId}
										onOpenFile={onOpenFile}
									/>
								</div>
							))
						: null}
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
