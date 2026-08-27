import { useVirtualizer } from "@tanstack/react-virtual";
import {
	Bot,
	Copy,
	FileEdit,
	FileText,
	GitBranch,
	TestTube2,
	User,
} from "lucide-react";
import { type ReactNode, useCallback, useLayoutEffect, useRef } from "react";
import { useTranslation } from "react-i18next";
import type {
	ChatMessage,
	OqtoUiPlatform,
	SessionOverview,
	SessionTask,
	UiNavigation,
	WorkArea,
	WorkDirectory,
} from "../platform/contracts";
import { Composer } from "./Composer";
import { TaskProgress } from "./TaskProgress";
import { EditorPane, TerminalPane, WorkAreaTabs } from "./WorkAreaPanes";
import { useTimeline } from "./useTimeline";

type ChatWorkspaceProps = {
	platform: Pick<OqtoUiPlatform, "id" | "loadMessages">;
	directory: WorkDirectory;
	session: SessionOverview;
	tasks: SessionTask[];
	workArea: WorkArea;
	workAreaTab: string;
	galleryPane: ReactNode;
	onNavigate: (next: UiNavigation) => void;
};

type ActivityKind = NonNullable<ChatMessage["activity"]>["kind"];

type ToolIconProps = {
	kind: ActivityKind;
};

function ToolIcon({ kind }: ToolIconProps) {
	if (kind === "edit") return <FileEdit aria-hidden="true" />;
	if (kind === "test") return <TestTube2 aria-hidden="true" />;
	return <FileText aria-hidden="true" />;
}

type MessageGroupProps = {
	message: ChatMessage;
	agentName: string;
};

function MessageGroup({ message, agentName }: MessageGroupProps) {
	const { t } = useTranslation();
	const isUser = message.author === "user";
	const name = isUser
		? t("oqtoUi.person.name")
		: message.author === "tool"
			? t("oqtoUi.chat.tool")
			: agentName;
	return (
		<article className="wb-msg" data-author={message.author}>
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

export function ChatWorkspace({
	platform,
	directory,
	session,
	tasks,
	workArea,
	workAreaTab,
	galleryPane,
	onNavigate,
}: ChatWorkspaceProps) {
	const { t } = useTranslation();
	const editorTab = workArea.tabs.find((tab) => tab.id === "editor");
	return (
		<main className="wb-main" aria-label={t("oqtoUi.chat.label")}>
			<div className="wb-chat-card">
				<WorkAreaTabs
					tabs={workArea.tabs}
					activeTab={workAreaTab}
					chatLabel={session.name}
					chatMeta={`${directory.name} [${session.id}] | ${session.updated}`}
					onNavigate={onNavigate}
				/>
				{workAreaTab === "editor" ? (
					<EditorPane
						fileName={editorTab?.fileName ?? ""}
						lines={workArea.editorLines}
					/>
				) : null}
				{workAreaTab === "terminal" ? (
					<TerminalPane lines={workArea.terminalLines} />
				) : null}
				{workAreaTab === "gallery" ? galleryPane : null}
				{workAreaTab !== "editor" &&
				workAreaTab !== "terminal" &&
				workAreaTab !== "gallery" ? (
					<ChatPane
						platform={platform}
						directory={directory}
						sessionId={session.id}
						tasks={tasks}
					/>
				) : null}
			</div>
		</main>
	);
}

type ChatPaneProps = {
	platform: Pick<OqtoUiPlatform, "id" | "loadMessages">;
	directory: WorkDirectory;
	sessionId: string;
	tasks: SessionTask[];
};

function ChatPane({ platform, directory, sessionId, tasks }: ChatPaneProps) {
	const { t } = useTranslation();
	const timeline = useTimeline(platform, sessionId);
	const scrollRef = useRef<HTMLElement | null>(null);
	// Viewport anchoring: keep the visible content stable when an earlier page
	// prepends, and follow the tail until the user scrolls away from it.
	const anchorRef = useRef({
		totalSize: 0,
		sessionId,
		firstMessageId: null as string | null,
	});
	const followTailRef = useRef(true);

	// Stable identity matters: the virtualizer memoizes on getItemKey, and a
	// fresh closure per render would notify-and-rerender forever.
	const getItemKey = useCallback(
		(index: number) => timeline.messages[index]?.id ?? index,
		[timeline.messages],
	);

	const virtualizer = useVirtualizer({
		count: timeline.messages.length,
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
		const totalSize = virtualizer.getTotalSize();
		const grew = totalSize - anchor.totalSize;
		// Only touch scrollTop when the content actually changed. Writing it
		// on every commit (remeasures, unrelated queries) fights the user's
		// momentum and reads as stutter while scrolling.
		if (grew === 0) return;
		// Crucially, mere size drift (mobile URL-bar collapse, keyboard,
		// viewport resize re-measures) must NOT shift the reading position;
		// only a real prepended page may move the offset by its growth.
		const prepended =
			firstMessageId !== null && anchor.firstMessageId !== firstMessageId;
		anchorRef.current = { totalSize, sessionId, firstMessageId };
		if (followTailRef.current) {
			// Follow the newest content until the user scrolls up.
			element.scrollTop = element.scrollHeight;
		} else if (prepended && grew > 0) {
			// An earlier page prepended: shift the viewport by the growth so
			// the messages the user was reading stay in place.
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
					className="wb-timeline"
					style={
						{
							"--timeline-size": `${virtualizer.getTotalSize()}px`,
						} as React.CSSProperties
					}
				>
					{visibleItems.map((row) => {
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
								<MessageGroup agentName={directory.name} message={message} />
							</div>
						);
					})}
				</div>
			</section>

			<TaskProgress tasks={tasks} placement="desktop" />

			<Composer />
		</>
	);
}
