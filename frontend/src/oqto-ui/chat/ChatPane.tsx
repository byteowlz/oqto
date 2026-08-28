import { useIsMobile } from "@/hooks/use-mobile";
import { useMountEffect } from "@/hooks/use-mount-effect";
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
import { useCallback, useLayoutEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import type { ChatTurnDraft } from "../platform/chat-contract";
import type {
	ChatMessage,
	OqtoUiPlatform,
	SessionTask,
} from "../platform/contracts";
import { TaskProgress } from "./TaskProgress";
import { timelineQueryKey, turnDraftQueryKey } from "./query-keys";
import { useTimeline } from "./useTimeline";

type ChatPaneProps = {
	platform: Pick<OqtoUiPlatform, "id" | "loadMessages" | "chat">;
	agentName: string;
	sessionId: string;
	tasks: SessionTask[];
};

type PendingPrompt = { id: string; text: string };

type ActivityKind = NonNullable<ChatMessage["activity"]>["kind"];

type ToolIconProps = { kind: ActivityKind };

function ToolIcon({ kind }: ToolIconProps) {
	if (kind === "edit") return <FileEdit aria-hidden="true" />;
	if (kind === "test") return <TestTube2 aria-hidden="true" />;
	return <FileText aria-hidden="true" />;
}

type MessageLike = Pick<ChatMessage, "author" | "content" | "time"> & {
	activity?: ChatMessage["activity"];
};

type MessageGroupProps = {
	message: MessageLike;
	agentName: string;
	pending?: boolean;
};

function MessageGroup({ message, agentName, pending }: MessageGroupProps) {
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
				<p>{message.content}</p>
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

type DraftPartViewProps = { part: ChatTurnDraft["parts"][number] };

function DraftPartView({ part }: DraftPartViewProps) {
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
					<p>{part.text}</p>
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
		return (
			<div className="wb-tool wb-draft-tool" data-status={part.status}>
				<FileEdit aria-hidden="true" />
				<span className="wb-tool__label">{part.name}</span>
			</div>
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
		(index: number) => timeline.messages[index]?.id ?? index,
		[timeline.messages],
	);

	const virtualizer = useVirtualizer({
		count: compact ? 0 : timeline.messages.length,
		getScrollElement: () => scrollRef.current,
		// Close to the real average row height; large misestimates make the
		// scrollbar and viewport visibly jump when rows measure.
		estimateSize: () => 76,
		overscan: 5,
		getItemKey,
		isScrollingResetDelay: 150,
	});

	const firstMessageId = timeline.messages[0]?.id ?? null;
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
						? timeline.messages.map((message) => (
								<div className="wb-timeline-row" key={message.id}>
									<MessageGroup agentName={agentName} message={message} />
								</div>
							))
						: visibleItems.map((row) => {
								const message = timeline.messages[row.index];
								if (!message) return null;
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
										<MessageGroup agentName={agentName} message={message} />
									</div>
								);
							})}
					{pendingPrompt ? (
						<div className="wb-timeline-row" key={pendingPrompt.id}>
							<MessageGroup
								agentName={agentName}
								message={{
									author: "user",
									content: pendingPrompt.text,
									time: t("oqtoUi.chat.pending"),
								}}
								pending
							/>
						</div>
					) : null}
					{draft
						? draft.parts.map((part) => (
								<div className="wb-timeline-row" key={part.id}>
									<DraftPartView part={part} />
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
