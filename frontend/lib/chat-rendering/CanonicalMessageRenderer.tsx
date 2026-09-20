import { A2UICallCard } from "@/components/chat/a2ui-call-card";
import { ToolCallCard, getToolIcon } from "@/components/chat/tool-call-card";
import { ToolCallGroup } from "@/components/chat/tool-call-group";
import { BrailleSpinner } from "@/components/common";
import { MarkdownRenderer } from "@/components/data-display";
import type { MarkdownImage } from "@/components/data-display/markdown-renderer";
import {
	ContextMenu,
	ContextMenuContent,
	ContextMenuItem,
	ContextMenuTrigger,
} from "@/components/ui/context-menu";
import { AgentErrorBody } from "@/features/chat/components/AgentErrorBody";
import type { A2UIUserAction } from "@/lib/a2ui/types";
import type { FileRange, Part } from "@/lib/canonical-types";
import type { A2UISurfaceState, DisplayPart } from "@/lib/chat-render-types";
import { useChatVerbosity } from "@/lib/chat-verbosity";
import { extractFileReferenceDetails, getFileTypeInfo } from "@/lib/file-types";
import type { StreamingPresentationMode } from "@/lib/streaming-presentation";
import { getToolSummary } from "@/lib/tool-summaries";
import { cn } from "@/lib/utils";
import {
	Check,
	Copy,
	Download,
	ExternalLink,
	FileCode,
	FileImage,
	FileText,
	FileVideo,
	GitBranch,
	Loader2,
	PaintBucket,
	Paperclip,
} from "lucide-react";
import {
	type ReactNode,
	createContext,
	memo,
	useCallback,
	useContext,
	useEffect,
	useLayoutEffect,
	useMemo,
	useRef,
	useState,
} from "react";
import { useTranslation } from "react-i18next";
import { dedentMarkdown, stripAnsiSequences } from "./chat-render-text";
import type { MessageGroup } from "./group-messages";

/**
 * The one action people actually reach for, so it is always on screen rather
 * than revealed under the pointer. It carries no size utilities of its own:
 * the chat stylesheet owns the metrics, which keeps it on the byline's scale
 * whatever the reader has chosen for the prose.
 */
function ChatCopyAction({ text, label }: { text: string; label: string }) {
	const [copied, setCopied] = useState(false);

	const handleCopy = useCallback(() => {
		try {
			if (navigator.clipboard?.writeText) {
				navigator.clipboard.writeText(text);
			} else {
				const textArea = document.createElement("textarea");
				textArea.value = text;
				textArea.style.position = "fixed";
				textArea.style.left = "-9999px";
				document.body.appendChild(textArea);
				textArea.select();
				document.execCommand("copy");
				document.body.removeChild(textArea);
			}
			setCopied(true);
			setTimeout(() => setCopied(false), 2000);
		} catch {}
	}, [text]);

	return (
		<button
			type="button"
			onClick={handleCopy}
			className="chat-action"
			title={label}
		>
			{copied ? <Check aria-hidden="true" /> : <Copy aria-hidden="true" />}
			<span>{label}</span>
		</button>
	);
}

type Segment =
	| { key: string; type: "text"; text: string; timestamp: number }
	| {
			key: string;
			type: "tool_call";
			part: Extract<DisplayPart, { type: "tool_call" }>;
			toolResult?: Extract<DisplayPart, { type: "tool_result" }>;
			timestamp: number;
	  }
	| {
			key: string;
			type: "tool_result_only";
			part: Extract<DisplayPart, { type: "tool_result" }>;
			timestamp: number;
	  }
	| { key: string; type: "thinking"; text: string; timestamp: number }
	| { key: string; type: "compaction"; text: string; timestamp: number }
	| { key: string; type: "part"; part: DisplayPart; timestamp: number }
	| {
			key: string;
			type: "error";
			text: string;
			timestamp: number;
			retrying?: boolean;
			retryAttempt?: number;
			retryMax?: number;
	  }
	| {
			key: string;
			type: "a2ui";
			surface: A2UISurfaceState;
			timestamp: number;
	  };

/** Gutter icon for collapsed tool calls in minimal mode (verbosity=1).
 *  Shows a single collapsed icon; click opens a popover listing each tool + summary. */
/**
 * Everything the agent did, condensed to one quiet line.
 *
 * This is what the lowest detail level shows in place of thinking blocks and
 * tool rows: a count, the icons of the tools involved, and — once expanded —
 * the same plain-language summaries the tool rows carry. It sits in the flow
 * above the answer rather than in a margin, because a margin is not reachable
 * on a narrow viewport and it competes with the reading measure.
 */
function ChatActivitySummary({
	segments,
	locale,
	preambleOf,
}: {
	segments: Array<
		| Extract<Segment, { type: "tool_call" }>
		| Extract<Segment, { type: "tool_result_only" }>
	>;
	locale: "en" | "de";
	/** What the agent said it was about to do, keyed by segment. */
	preambleOf: Map<string, string>;
}) {
	const { t } = useTranslation();

	// Consecutive calls to the same tool read as one step — unless the agent
	// announced the second one, in which case it said itself that this is a
	// new step and it gets its own line.
	const steps: Array<{
		toolName: string;
		count: number;
		input?: Record<string, unknown>;
		said?: string;
	}> = [];
	for (const segment of segments) {
		const toolName =
			segment.type === "tool_call"
				? segment.part.name
				: segment.part.name || "result";
		const input =
			segment.type === "tool_call"
				? (segment.part.input as Record<string, unknown> | undefined)
				: undefined;
		const said = preambleOf.get(segment.key);
		const last = steps[steps.length - 1];
		if (last && last.toolName === toolName && said === undefined) {
			last.count += 1;
			continue;
		}
		steps.push({ toolName, count: 1, input, said });
	}

	if (steps.length === 0) return null;

	// A handful of icons says "some work happened"; thirty says nothing.
	const shownIcons = steps.slice(0, 3);

	return (
		<details className="chat-activity">
			<summary>
				<span className="chat-activity__icons">
					{shownIcons.map((step, index) => (
						<span key={`${step.toolName}-${index}`}>
							{getToolIcon(step.toolName, step.input)}
						</span>
					))}
				</span>
				<span className="chat-activity__count">
					{t("chat.activitySteps", { count: segments.length })}
				</span>
			</summary>
			<ol>
				{steps.map((step, index) => (
					<li key={`${step.toolName}-${index}`}>
						{step.said ??
							getToolSummary(step.toolName, step.input, locale) ??
							step.toolName}
						{step.count > 1 ? (
							<span className="opacity-60"> x{step.count}</span>
						) : null}
					</li>
				))}
			</ol>
		</details>
	);
}

export type ChatFileAdapter = {
	fileUrl: (workspacePath: string, filePath: string) => string;
	/**
	 * Read the answer aloud. Nothing renders it at the moment: speech is not
	 * wired up, so the button was offering something that does not happen.
	 * The seam stays because the adapter is where a host declares what it can
	 * do, and this is a host capability, not a renderer one.
	 */
	renderReadAloud?: (text: string) => ReactNode;
	downloadFile?: (
		workspacePath: string,
		filePath: string,
		fileName: string,
	) => Promise<void>;
};

/** Longest single line still read as an announcement rather than an answer. */
const PREAMBLE_MAX = 160;

/**
 * Drops the lines that are nothing but file mentions.
 *
 * Attaching files to a prompt writes them into the text as "@uploads/x.pdf"
 * tokens, so a message with seven attachments opens with seven paths and then
 * says what it actually wanted — and the same seven files are listed again
 * underneath as chips. The tokens are how the attachment reaches the agent,
 * not something the reader wrote or needs to see.
 *
 * Only whole lines go. A mention inside a sentence ("look at @src/foo.ts and
 * tell me why") is the writer referring to a file, and it stays.
 */
function withoutMentionOnlyLines(content: string): string {
	let inFence = false;
	const kept: string[] = [];
	for (const line of content.split("\n")) {
		if (/^\s*(?:```|~~~)/.test(line)) {
			inFence = !inFence;
			kept.push(line);
			continue;
		}
		const isManifest =
			!inFence &&
			line.trim().length > 0 &&
			line
				.trim()
				.split(/\s+/)
				.every((token) => /^@[^\s@`"'<>()[\]{}]+$/.test(token));
		if (isManifest) continue;
		kept.push(line);
	}
	return kept.join("\n").replace(/^\n+/, "");
}

const ChatFileAdapterContext = createContext<ChatFileAdapter | null>(null);
type FileReferenceOpenHandler = (filePath: string, range?: FileRange) => void;
/** Which image was activated, and the others in the same message. */
export type ImageOpenHandler = (
	image: MarkdownImage,
	siblings: MarkdownImage[],
) => void;
const FileReferenceOpenContext = createContext<
	FileReferenceOpenHandler | undefined
>(undefined);

export const MessageGroupCard = memo(function MessageGroupCard({
	group,
	assistantName,
	tempIdLabel,
	workspacePath,
	locale = "en",
	a2uiSurfaces = [],
	onA2UIAction,
	messageId,
	showWorkingIndicator = false,
	onForkHere,
	onFileReferenceOpen,
	onImageOpen,
	hideRecoveredErrors = false,
	freezeStreamingUpdates = false,
	streamingPresentationMode = "raw",
	fileAdapter = null,
}: {
	group: MessageGroup;
	assistantName?: string | null;
	tempIdLabel?: string | null;
	workspacePath?: string | null;
	locale?: "de" | "en";
	a2uiSurfaces?: A2UISurfaceState[];
	onA2UIAction?: (action: import("@/lib/a2ui/types").A2UIUserAction) => void;
	messageId?: string;
	showWorkingIndicator?: boolean;
	onForkHere?: () => void;
	onFileReferenceOpen?: FileReferenceOpenHandler;
	/** Enlarge an image the reader clicked; the host decides where. */
	onImageOpen?: ImageOpenHandler;
	hideRecoveredErrors?: boolean;
	freezeStreamingUpdates?: boolean;
	streamingPresentationMode?: StreamingPresentationMode;
	fileAdapter?: ChatFileAdapter | null;
}) {
	const isUser = group.role === "user";
	const { t } = useTranslation();
	const { verbosity } = useChatVerbosity();
	const createdAt = group.messages[0]?.timestamp
		? new Date(group.messages[0].timestamp)
		: null;

	// Flatten parts in order, preserve message-array ordering.
	// The primary sort key is the message index (msgIndex), NOT the wall-clock
	// timestamp.  Timestamps from oqto-log (created_at) can be non-monotonic when
	// messages are persisted out of chronological order (e.g. batched writes).
	// Using them as a sort key reorders messages incorrectly.
	type TimedPart = {
		key: string;
		part: DisplayPart;
		/** Sort order: msgIndex * 1e6 + partIndex preserves array order. */
		timestamp: number;
	};

	const timedParts: TimedPart[] = [];
	for (const [msgIndex, message] of group.messages.entries()) {
		for (const [partIndex, part] of message.parts.entries()) {
			timedParts.push({
				key: `${message.id}-${msgIndex}-${part.type}-${partIndex}`,
				part,
				timestamp: msgIndex * 1_000_000 + partIndex,
			});
		}
	}

	const toolResults = new Map<
		string,
		Extract<DisplayPart, { type: "tool_result" }>
	>();
	const toolCallIds = new Set<string>();
	for (const { part } of timedParts) {
		if (part.type === "tool_result") {
			toolResults.set(part.toolCallId, part);
		}
		if (part.type === "tool_call") {
			toolCallIds.add(part.toolCallId);
		}
	}

	const segments: Segment[] = [];
	let textBuf: string[] = [];
	let textKey: string | null = null;
	let lastTs = 0;
	const flushText = () => {
		if (textBuf.length === 0) return;
		segments.push({
			key: textKey ?? `text-${segments.length}`,
			type: "text",
			text: textBuf.join("\n\n"),
			timestamp: lastTs,
		});
		textBuf = [];
		textKey = null;
	};

	for (const { key, part, timestamp } of timedParts) {
		lastTs = timestamp;
		if (part.type === "text") {
			// Safely resolve text content (might be .text or .content depending on source)
			const partText =
				(part as { text?: string; content?: string }).text ??
				(part as { content?: string }).content ??
				"";
			// Skip empty chunks to avoid flicker
			if (!partText.trim()) continue;
			// Skip text that looks like raw tool JSON output
			const trimmedText = partText.trim();
			// Skip question tool JSON
			const looksLikeQuestionJson =
				trimmedText.startsWith("{") &&
				trimmedText.includes('"questions"') &&
				trimmedText.includes('"header"') &&
				trimmedText.includes('"options"');
			if (looksLikeQuestionJson) continue;
			// Skip raw command JSON like {"command":"ls -la"} or {"filePath":"..."}
			const looksLikeCommandJson =
				trimmedText.startsWith("{") &&
				trimmedText.endsWith("}") &&
				(trimmedText.includes('"command"') ||
					trimmedText.includes('"filePath"') ||
					trimmedText.includes('"pattern"') ||
					trimmedText.includes('"content"'));
			if (looksLikeCommandJson && trimmedText.length < 500) continue;
			if (!textKey) textKey = key;
			textBuf.push(partText);
			continue;
		}

		flushText();

		if (part.type === "tool_call") {
			const matchedResult = toolResults.get(part.toolCallId);
			segments.push({
				key,
				type: "tool_call",
				part,
				toolResult: matchedResult,
				timestamp,
			});
		} else if (part.type === "tool_result") {
			// Render only if we don't have a corresponding tool_call
			if (!toolCallIds.has(part.toolCallId)) {
				segments.push({
					key,
					type: "tool_result_only",
					part,
					timestamp,
				});
			}
		} else if (part.type === "thinking") {
			segments.push({
				key,
				type: "thinking",
				text: (part as { text?: string }).text ?? "",
				timestamp,
			});
		} else if (part.type === "error") {
			const ep = part as {
				text?: string;
				retrying?: boolean;
				retryAttempt?: number;
				retryMax?: number;
			};
			segments.push({
				key,
				type: "error",
				text: ep.text ?? "",
				timestamp,
				retrying: ep.retrying,
				retryAttempt: ep.retryAttempt,
				retryMax: ep.retryMax,
			});
		} else if (part.type === "compaction") {
			segments.push({
				key,
				type: "compaction",
				text: (part as { text?: string }).text ?? "",
				timestamp,
			});
		} else {
			segments.push({ key, type: "part", part, timestamp });
		}
	}
	flushText();

	// A2UI surfaces are appended after all message parts.
	// Give them indices that sort after regular parts.
	const surfaceBase = (group.messages.length + 1) * 1_000_000;
	for (const [surfIdx, surface] of a2uiSurfaces.entries()) {
		segments.push({
			key: `a2ui-${surface.surfaceId}`,
			type: "a2ui",
			surface,
			timestamp: surfaceBase + surfIdx,
		});
	}

	segments.sort((a, b) => a.timestamp - b.timestamp);

	const normalizedSegments: Segment[] = [];
	const seenToolCalls = new Set<string>();
	const seenStandaloneToolResults = new Set<string>();
	for (let i = 0; i < segments.length; i++) {
		const segment = segments[i];
		if (segment.type === "tool_call") {
			const toolCallId = segment.part.toolCallId;
			if (seenToolCalls.has(toolCallId)) {
				continue;
			}
			seenToolCalls.add(toolCallId);

			let toolResult = segment.toolResult;
			let consumeNextStandaloneResult = false;
			const next = segments[i + 1];
			if (next?.type === "tool_result_only") {
				if (!toolResult) {
					toolResult = next.part;
					consumeNextStandaloneResult = true;
				} else {
					const sameToolCallId = next.part.toolCallId === toolResult.toolCallId;
					if (sameToolCallId) {
						consumeNextStandaloneResult = true;
					}
				}
			}
			if (consumeNextStandaloneResult && next?.type === "tool_result_only") {
				seenStandaloneToolResults.add(next.part.toolCallId);
				i += 1;
			}
			normalizedSegments.push(
				toolResult && !segment.toolResult
					? { ...segment, toolResult }
					: segment,
			);
			continue;
		}
		if (segment.type === "tool_result_only") {
			const resultId = segment.part.toolCallId;
			if (
				seenStandaloneToolResults.has(resultId) ||
				seenToolCalls.has(resultId)
			) {
				continue;
			}
			seenStandaloneToolResults.add(resultId);
		}
		normalizedSegments.push(segment);
	}

	// Keep only the latest active retry card per assistant group.
	// Retry events can arrive across multiple message fragments; rendering all
	// fragments creates repeated cards (1/3, 2/3, 3/3). We keep the newest one.
	const latestRetryingErrorIndex = (() => {
		let latest = -1;
		for (let i = 0; i < normalizedSegments.length; i++) {
			const segment = normalizedSegments[i];
			if (segment.type === "error" && segment.retrying) {
				latest = i;
			}
		}
		return latest;
	})();

	const displaySegments = normalizedSegments.filter((segment, index) => {
		if (segment.type === "error" && segment.retrying) {
			return index === latestRetryingErrorIndex;
		}
		return true;
	});

	const hasNonErrorContinuationAfter = (index: number): boolean => {
		for (let i = index + 1; i < displaySegments.length; i++) {
			const next = displaySegments[i];
			if (next.type === "error") continue;
			if (next.type === "tool_call") {
				if (next.toolResult?.isError) continue;
				return true;
			}
			if (next.type === "tool_result_only") {
				if (next.part.isError) continue;
				return true;
			}
			return true;
		}
		return false;
	};

	const recoveredSegmentKeys = new Set<string>();
	for (const [index, segment] of displaySegments.entries()) {
		const continued = hasNonErrorContinuationAfter(index);
		if (!continued) continue;
		if (segment.type === "error" && !segment.retrying) {
			recoveredSegmentKeys.add(segment.key);
		}
		if (segment.type === "tool_call" && segment.toolResult?.isError) {
			recoveredSegmentKeys.add(segment.key);
		}
		if (segment.type === "tool_result_only" && segment.part.isError) {
			recoveredSegmentKeys.add(segment.key);
		}
	}

	type RenderSegment =
		| Segment
		| {
				key: string;
				type: "tool_group";
				segments: Array<
					| Extract<Segment, { type: "tool_call" }>
					| Extract<Segment, { type: "tool_result_only" }>
				>;
				timestamp: number;
		  };

	// An agent announces what it is about to do and then does it: "Let me
	// check how the frame reads downloaded files:" followed by a read. The
	// sentence is not part of the answer — it is the human-readable label of
	// the call that follows, and the colon is pointing straight at it.
	//
	// Pairing the two matters most at the lowest detail level, where lifting
	// the calls out of the flow used to strand a column of orphaned sentences
	// that all ended in a colon and introduced nothing. Paired, they travel
	// together: the narration goes wherever its call goes, and what stays in
	// the flow is the answer.
	const preambleOf = new Map<string, string>();
	const preambleKeys = new Set<string>();
	for (const [index, segment] of displaySegments.entries()) {
		if (segment.type !== "text") continue;
		const next = displaySegments[index + 1];
		if (next?.type !== "tool_call" && next?.type !== "tool_result_only") {
			continue;
		}
		const announcement = segment.text.trim();
		// Conservative on purpose: a colon, or one short line. A paragraph of
		// answer that happens to sit before a call stays in the answer.
		const announces =
			announcement.endsWith(":") ||
			(!announcement.includes("\n") && announcement.length <= PREAMBLE_MAX);
		if (!announces) continue;
		preambleOf.set(next.key, announcement.replace(/:$/, ""));
		preambleKeys.add(segment.key);
	}

	// At the lowest detail level the thinking blocks and the tool rows leave
	// the flow entirely and become one summary line above the answer. The
	// answer itself is untouched: a less technical reader gets the same text.
	const minimalActivity: Array<
		| Extract<Segment, { type: "tool_call" }>
		| Extract<Segment, { type: "tool_result_only" }>
	> =
		verbosity === 1
			? displaySegments.filter(
					(
						segment,
					): segment is
						| Extract<Segment, { type: "tool_call" }>
						| Extract<Segment, { type: "tool_result_only" }> =>
						segment.type === "tool_call" || segment.type === "tool_result_only",
				)
			: [];

	const renderSegments: RenderSegment[] = (() => {
		if (verbosity === 1) {
			const flow = displaySegments.filter(
				(segment) =>
					segment.type !== "thinking" &&
					segment.type !== "tool_call" &&
					segment.type !== "tool_result_only" &&
					!preambleKeys.has(segment.key),
			);
			// A turn that was nothing but announcements and calls would render
			// as an empty answer, so the last announcement stands in for one.
			if (flow.length > 0) return flow;
			const lastText = [...displaySegments]
				.reverse()
				.find((segment) => segment.type === "text");
			return lastText ? [lastText] : [];
		}
		if (verbosity !== 2) return displaySegments;
		const grouped: RenderSegment[] = [];
		let toolBuffer: Extract<RenderSegment, { type: "tool_group" }>["segments"] =
			[];
		let thinkingBuffer: string[] = [];
		let thinkingKey: string | null = null;
		let thinkingTimestamp = 0;

		const flushThinking = () => {
			if (thinkingBuffer.length === 0) return;
			grouped.push({
				key: thinkingKey ?? `thinking-${grouped.length}`,
				type: "thinking",
				text: thinkingBuffer.join("\n\n"),
				timestamp: thinkingTimestamp,
			});
			thinkingBuffer = [];
			thinkingKey = null;
			thinkingTimestamp = 0;
		};

		const flushTools = () => {
			if (toolBuffer.length === 0) return;
			if (toolBuffer.length === 1) {
				grouped.push(toolBuffer[0]);
			} else {
				grouped.push({
					key: `tool-group-${toolBuffer[0].key}`,
					type: "tool_group",
					segments: toolBuffer,
					timestamp: toolBuffer[0].timestamp,
				});
			}
			toolBuffer = [];
		};

		const flushRun = () => {
			flushThinking();
			flushTools();
		};

		for (const segment of displaySegments) {
			if (segment.type === "tool_call" || segment.type === "tool_result_only") {
				toolBuffer.push(segment);
				continue;
			}
			if (segment.type === "thinking") {
				if (thinkingBuffer.length === 0) {
					thinkingKey = segment.key;
					thinkingTimestamp = segment.timestamp;
				}
				thinkingBuffer.push(segment.text);
				continue;
			}
			flushRun();
			grouped.push(segment);
		}
		flushRun();
		return grouped;
	})();

	const allTextContent = segments
		.filter((s): s is Extract<Segment, { type: "text" }> => s.type === "text")
		.map((s) => s.text)
		.join("\n\n");
	const isGroupStreaming =
		!isUser && group.messages.some((message) => message.isStreaming === true);

	// Use workspace name instead of "Assistant" when assistantName is not provided
	const workspaceName = workspacePath?.split("/").pop() || "Assistant";
	const assistantDisplayName = assistantName || workspaceName;

	const messageCard = (
		<div
			data-message-id={messageId}
			className={cn(
				"chat-turn group min-w-0",
				isUser ? "chat-turn--user" : "chat-turn--agent",
			)}
		>
			<div className="chat-byline">
				<span className="chat-byline__who">
					{isUser
						? (group.messages[0]?.sender?.name ?? t("chat.you"))
						: assistantDisplayName}
				</span>
				{createdAt && !Number.isNaN(createdAt.getTime()) ? (
					<span>
						{createdAt.toLocaleTimeString([], {
							hour: "2-digit",
							minute: "2-digit",
						})}
					</span>
				) : null}
				<span className="chat-byline__spacer" />
				{isUser && onForkHere ? (
					<button
						type="button"
						className="chat-action"
						onClick={(event) => {
							event.stopPropagation();
							onForkHere();
						}}
						title={t("chat.forkHere", "Fork here")}
					>
						<GitBranch aria-hidden="true" />
					</button>
				) : null}
				{allTextContent ? (
					<ChatCopyAction text={allTextContent} label={t("chat.copy")} />
				) : null}
			</div>

			<div
				className={cn(
					"relative min-w-0 max-w-full space-y-2 overflow-hidden",
					isUser && "chat-bubble",
				)}
			>
				{renderSegments.length === 0 && !isUser && showWorkingIndicator && (
					<div className="flex items-center gap-3 text-muted-foreground text-sm">
						<BrailleSpinner />
						<span>{t("chat.working")}</span>
					</div>
				)}
				{renderSegments.length === 0 && isUser && (
					<span className="text-muted-foreground italic text-sm">
						No content
					</span>
				)}

				{renderSegments.map((segment, idx) => {
					const prevSegment = idx > 0 ? renderSegments[idx - 1] : null;
					const needsTopMargin =
						prevSegment?.type === "text" && segment.type !== "text";

					if (segment.type === "text") {
						const inner = (
							<TextWithFileReferences
								key={segment.key}
								content={segment.text}
								workspacePath={workspacePath}
								locale={locale}
								onFileReferenceOpen={onFileReferenceOpen}
								onImageOpen={onImageOpen}
								deferMermaidUntilFinal={
									!isUser && (showWorkingIndicator || isGroupStreaming)
								}
								isStreaming={
									!isUser && (showWorkingIndicator || isGroupStreaming)
								}
								freezeStreamingUpdates={freezeStreamingUpdates}
								streamingPresentationMode={streamingPresentationMode}
							/>
						);
						return inner;
					}
					if (segment.type === "tool_call") {
						const isRecoveredError = recoveredSegmentKeys.has(segment.key);
						const hasToolError =
							segment.part.status === "error" ||
							Boolean(segment.toolResult?.isError);
						if (hideRecoveredErrors && (isRecoveredError || hasToolError)) {
							return null;
						}
						return (
							<div
								key={segment.key}
								className={needsTopMargin ? "mt-2" : undefined}
							>
								<PiPartRenderer
									part={segment.part}
									toolResult={segment.toolResult}
									locale={locale}
									workspacePath={workspacePath}
									streamingPresentationMode={streamingPresentationMode}
									isRecoveredError={isRecoveredError}
								/>
							</div>
						);
					}
					if (segment.type === "tool_result_only") {
						const isRecoveredError = recoveredSegmentKeys.has(segment.key);
						const hasToolError = Boolean(segment.part.isError);
						if (hideRecoveredErrors && (isRecoveredError || hasToolError)) {
							return null;
						}
						// Render standalone tool result (no matching tool_use found)
						return (
							<div
								key={segment.key}
								className={needsTopMargin ? "mt-2" : undefined}
							>
								<PiPartRenderer
									part={segment.part}
									locale={locale}
									workspacePath={workspacePath}
									streamingPresentationMode={streamingPresentationMode}
									isRecoveredError={isRecoveredError}
								/>
							</div>
						);
					}
					if (segment.type === "thinking") {
						const inner = (
							<div
								key={segment.key}
								className={needsTopMargin ? "mt-3" : undefined}
							>
								<PiPartRenderer
									part={
										{
											type: "thinking",
											id: segment.key,
											text: segment.text,
											__verbosity: verbosity,
										} as DisplayPart
									}
									locale={locale}
									workspacePath={workspacePath}
									isStreaming={!isUser && isGroupStreaming}
									freezeStreamingUpdates={freezeStreamingUpdates}
									streamingPresentationMode={streamingPresentationMode}
								/>
							</div>
						);
						return inner;
					}
					if (segment.type === "part") {
						return (
							<div
								key={segment.key}
								className={needsTopMargin ? "mt-2" : undefined}
							>
								<PiPartRenderer
									part={segment.part}
									locale={locale}
									workspacePath={workspacePath}
								/>
							</div>
						);
					}
					if (segment.type === "error") {
						const isRetrying = segment.retrying;
						const isRecoveredError = recoveredSegmentKeys.has(segment.key);
						if (hideRecoveredErrors && !isRetrying) {
							return null;
						}
						return (
							<div
								key={segment.key}
								className={cn(
									"rounded-md border px-3 py-2 text-sm flex gap-2",
									isRetrying ? "items-center" : "items-start",
									isRetrying
										? "border-amber-500/30 bg-amber-500/10 text-amber-600"
										: isRecoveredError
											? "border-amber-500/20 bg-amber-500/5 text-amber-700 dark:text-amber-400"
											: "border-destructive/20 bg-destructive/5 text-foreground/75",
									needsTopMargin && "mt-3",
								)}
							>
								{isRetrying && (
									<svg
										className="animate-spin h-3.5 w-3.5 shrink-0"
										viewBox="0 0 24 24"
										fill="none"
										aria-hidden="true"
									>
										<circle
											className="opacity-25"
											cx="12"
											cy="12"
											r="10"
											stroke="currentColor"
											strokeWidth="4"
										/>
										<path
											className="opacity-75"
											fill="currentColor"
											d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z"
										/>
									</svg>
								)}
								{isRetrying ? (
									<span>{segment.text}</span>
								) : (
									<AgentErrorBody text={segment.text} />
								)}
							</div>
						);
					}
					if (segment.type === "compaction") {
						const isLoading = segment.text === "Compacting context...";
						return (
							<div
								key={segment.key}
								className={cn(
									"flex items-center gap-2 my-3 text-xs text-muted-foreground",
								)}
							>
								<div className="flex-1 h-px bg-border" />
								<div className="flex items-center gap-1.5 px-2 py-1 rounded-full bg-muted/50 border border-border/50">
									{isLoading ? (
										<svg
											className="animate-spin h-3 w-3"
											viewBox="0 0 24 24"
											fill="none"
											aria-hidden="true"
										>
											<circle
												className="opacity-25"
												cx="12"
												cy="12"
												r="10"
												stroke="currentColor"
												strokeWidth="4"
											/>
											<path
												className="opacity-75"
												fill="currentColor"
												d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"
											/>
										</svg>
									) : (
										<svg
											className="h-3 w-3"
											viewBox="0 0 24 24"
											fill="none"
											stroke="currentColor"
											aria-hidden="true"
											strokeWidth="2"
										>
											<path d="M21 12a9 9 0 01-9 9m9-9a9 9 0 00-9-9m9 9H3m9 9a9 9 0 01-9-9m9 9c1.657 0 3-4.03 3-9s-1.343-9-3-9m0 18c-1.657 0-3-4.03-3-9s1.343-9 3-9" />
										</svg>
									)}
									<span>{segment.text}</span>
								</div>
								<div className="flex-1 h-px bg-border" />
							</div>
						);
					}
					if (segment.type === "tool_group") {
						const visibleToolSegments = hideRecoveredErrors
							? segment.segments.filter(
									(toolSegment) => !recoveredSegmentKeys.has(toolSegment.key),
								)
							: segment.segments;
						if (visibleToolSegments.length === 0) {
							return null;
						}
						const toolItems = visibleToolSegments.map((toolSegment) => {
							const toolName =
								toolSegment.type === "tool_call"
									? toolSegment.part.name
									: toolSegment.part.name || "result";
							const input =
								toolSegment.type === "tool_call"
									? (toolSegment.part.input as
											| Record<string, unknown>
											| undefined)
									: undefined;
							const summary = getToolSummary(toolName, input, locale);
							return {
								id: toolSegment.key,
								label: summary ?? toolName,
								icon: getToolIcon(toolName, input),
								render: () => (
									<PiPartRenderer
										part={toolSegment.part}
										toolResult={
											toolSegment.type === "tool_call"
												? toolSegment.toolResult
												: undefined
										}
										locale={locale}
										workspacePath={workspacePath}
										collapsible={false}
										hideHeader={verbosity === 2}
										isRecoveredError={recoveredSegmentKeys.has(toolSegment.key)}
									/>
								),
							};
						});
						return (
							<div
								key={segment.key}
								className={needsTopMargin ? "mt-2" : undefined}
							>
								<ToolCallGroup
									key={`${segment.key}-verbosity-${verbosity}`}
									mode="tabs"
									items={toolItems}
								/>
							</div>
						);
					}
					if (segment.type === "a2ui") {
						return (
							<div
								key={segment.key}
								className={needsTopMargin ? "mt-2" : undefined}
							>
								<A2UICallCard
									surfaceId={segment.surface.surfaceId}
									messages={segment.surface.messages}
									blocking={segment.surface.blocking}
									requestId={segment.surface.requestId}
									answered={segment.surface.answered}
									answeredAction={segment.surface.answeredAction}
									answeredAt={segment.surface.answeredAt}
									onAction={onA2UIAction}
									defaultCollapsed={segment.surface.answered}
								/>
							</div>
						);
					}
					return null;
				})}
			</div>

			{verbosity === 1 && minimalActivity.length > 0 ? (
				<ChatActivitySummary
					segments={minimalActivity}
					locale={locale}
					preambleOf={preambleOf}
				/>
			) : null}
		</div>
	);

	const isCoarsePointerDevice = useCoarsePointerDevice();
	if (isCoarsePointerDevice) {
		return (
			<FileReferenceOpenContext.Provider value={onFileReferenceOpen}>
				<ChatFileAdapterContext.Provider value={fileAdapter}>
					{messageCard}
				</ChatFileAdapterContext.Provider>
			</FileReferenceOpenContext.Provider>
		);
	}

	return (
		<FileReferenceOpenContext.Provider value={onFileReferenceOpen}>
			<ChatFileAdapterContext.Provider value={fileAdapter}>
				<ContextMenu>
					<ContextMenuTrigger className="contents">
						{messageCard}
					</ContextMenuTrigger>
					<ContextMenuContent>
						{isUser && onForkHere && (
							<ContextMenuItem onClick={() => onForkHere()} className="gap-2">
								<GitBranch className="w-4 h-4" />
								{t("chat.forkHere", "Fork here")}
							</ContextMenuItem>
						)}
						{allTextContent && (
							<ContextMenuItem
								onClick={() => navigator.clipboard?.writeText(allTextContent)}
								className="gap-2"
							>
								<Copy className="w-4 h-4" />
								{t("chat.copyAll")}
							</ContextMenuItem>
						)}
					</ContextMenuContent>
				</ContextMenu>
			</ChatFileAdapterContext.Provider>
		</FileReferenceOpenContext.Provider>
	);
});

/**
 * Renders a single part of a Pi message.
 */
function PiPartRenderer({
	part,
	toolResult,
	locale,
	workspacePath,
	collapsible = true,
	hideHeader = false,
	isStreaming = false,
	freezeStreamingUpdates = false,
	streamingPresentationMode = "raw",
	isRecoveredError = false,
}: {
	part: DisplayPart;
	toolResult?: Extract<DisplayPart, { type: "tool_result" }>;
	locale: "en" | "de";
	workspacePath?: string | null;
	collapsible?: boolean;
	hideHeader?: boolean;
	isStreaming?: boolean;
	freezeStreamingUpdates?: boolean;
	streamingPresentationMode?: StreamingPresentationMode;
	isRecoveredError?: boolean;
}) {
	const { t } = useTranslation();
	const stripAnsi = (value: string): string => stripAnsiSequences(value);
	const isThinkingPart = part.type === "thinking";
	const rawThinkingText = isThinkingPart
		? stripAnsi(((part as { text?: string }).text ?? "").trim())
		: "";
	const frozenThinkingText = useFrozenStreamingText(
		rawThinkingText,
		freezeStreamingUpdates && isStreaming && isThinkingPart,
	);
	const thinkingCrawlRef = useRef<HTMLDivElement>(null);
	const {
		visibleContent: visibleThinkingContent,
		isCommitAnimating: isThinkingCommitAnimating,
		animationPhase: thinkingAnimationPhase,
	} = useStreamingCommittedContent(
		frozenThinkingText,
		isStreaming && isThinkingPart,
		streamingPresentationMode,
	);
	useSmoothContainerHeight(
		thinkingCrawlRef,
		visibleThinkingContent,
		isStreaming &&
			isThinkingPart &&
			streamingPresentationMode === "smooth" &&
			!freezeStreamingUpdates,
	);
	useSmoothCrawlTransform(
		thinkingCrawlRef,
		visibleThinkingContent,
		isStreaming &&
			isThinkingPart &&
			streamingPresentationMode === "smooth" &&
			!freezeStreamingUpdates,
	);
	const thinkingReservedLines = useMemo(
		() =>
			isStreaming && isThinkingPart && streamingPresentationMode === "chunked"
				? estimateStreamingReservedLines(frozenThinkingText)
				: 0,
		[
			isStreaming,
			isThinkingPart,
			frozenThinkingText,
			streamingPresentationMode,
		],
	);

	const formatToolResultOutput = (content: unknown): string | undefined => {
		const decodeBytes = (bytes: Uint8Array): string | undefined => {
			try {
				const text = new TextDecoder("utf-8", { fatal: false }).decode(bytes);
				const printable = text
					.split("")
					.filter(
						(ch) => ch === "\n" || ch === "\r" || ch === "\t" || ch >= " ",
					).length;
				if (text.length === 0) return "";
				if (printable / text.length < 0.7) {
					return undefined;
				}
				return text;
			} catch {
				return undefined;
			}
		};

		if (typeof content === "string") {
			// Try to parse the string as JSON — some tools (e.g. MCP) serialize
			// their output as a JSON string containing an array of content blocks
			// like [{"type":"tool_result","output":"..."}] or [{"type":"text","text":"..."}].
			// In that case we recurse to extract the readable text instead of
			// rendering raw JSON.
			const trimmed = content.trim();
			if (trimmed.startsWith("[") || trimmed.startsWith("{")) {
				try {
					const parsed: unknown = JSON.parse(trimmed);
					if (Array.isArray(parsed) || (parsed && typeof parsed === "object")) {
						const extracted = formatToolResultOutput(parsed);
						if (extracted !== undefined) return extracted;
					}
				} catch {
					// not valid JSON — fall through to return the raw string
				}
			}
			return stripAnsi(content);
		}
		if (content instanceof Uint8Array) {
			return (
				decodeBytes(content) ?? `[binary data: ${content.byteLength} bytes]`
			);
		}
		if (Array.isArray(content)) {
			const byteArray =
				content.length > 0 && content.every((item) => typeof item === "number")
					? new Uint8Array(content as number[])
					: null;
			if (byteArray) {
				return (
					decodeBytes(byteArray) ??
					`[binary data: ${byteArray.byteLength} bytes]`
				);
			}
			const textBlocks = content
				.map((block) => {
					if (typeof block === "string") return block;
					if (!block || typeof block !== "object") return null;
					const b = block as Record<string, unknown>;
					if (b.type === "text" && typeof b.text === "string") return b.text;
					// Anthropic API tool_result block: { type: "tool_result", content: [...] }
					// Also handles MCP wrappers: { type: "tool_result", output: "..." }
					if (
						b.type === "tool_result" ||
						b.type === "toolResult" ||
						b.type === "tool_use"
					) {
						const inner =
							"content" in b ? b.content : "output" in b ? b.output : undefined;
						if (inner !== undefined) {
							return formatToolResultOutput(inner) ?? null;
						}
					}
					return null;
				})
				.filter((text): text is string => Boolean(text));
			if (textBlocks.length > 0) {
				return stripAnsi(textBlocks.join("\n\n"));
			}
		}
		if (content && typeof content === "object") {
			const obj = content as Record<string, unknown>;
			const base64 =
				(typeof obj.data_base64 === "string" && obj.data_base64) ||
				(typeof obj.content_base64 === "string" && obj.content_base64) ||
				(typeof obj.stdout_base64 === "string" && obj.stdout_base64) ||
				(typeof obj.bytes_base64 === "string" && obj.bytes_base64);
			if (base64) {
				try {
					const bytes = Uint8Array.from(atob(base64), (c) => c.charCodeAt(0));
					const decoded = decodeBytes(bytes);
					return decoded ?? `[binary data: ${bytes.byteLength} bytes]`;
				} catch {
					// fall through
				}
			}
			if (Array.isArray(obj.bytes)) {
				const bytes = new Uint8Array(
					obj.bytes.filter((item) => typeof item === "number") as number[],
				);
				return decodeBytes(bytes) ?? `[binary data: ${bytes.byteLength} bytes]`;
			}
			if (typeof obj.text === "string") return stripAnsi(obj.text);
			if (Array.isArray(obj.content)) {
				const nestedText = formatToolResultOutput(obj.content);
				if (nestedText) return nestedText;
			}
		}
		try {
			return stripAnsi(JSON.stringify(content, null, 2));
		} catch {
			return stripAnsi(String(content));
		}
	};

	switch (part.type) {
		case "text": {
			const textContent =
				(part as { text?: string; content?: string }).text ??
				(part as { content?: string }).content ??
				"";
			return (
				<TextWithFileReferences
					content={stripAnsi(textContent)}
					workspacePath={workspacePath}
					locale={locale}
					isStreaming={isStreaming}
					freezeStreamingUpdates={freezeStreamingUpdates}
					streamingPresentationMode={streamingPresentationMode}
				/>
			);
		}

		case "thinking": {
			const thinkingText = visibleThinkingContent;
			if (!thinkingText) return null;

			const verbosityLevel =
				typeof (part as Record<string, unknown>).__verbosity === "number"
					? ((part as Record<string, unknown>).__verbosity as 1 | 2 | 3)
					: 3;

			// Verbosity 3 (verbose): open by default, full markdown
			// Verbosity 2 (normal): collapsed, first-line preview
			// Verbosity 1 (compact): collapsed, just "Thinking" label
			const isOpen = verbosityLevel >= 3;
			// Always show first sentence preview when collapsed
			const firstSentence = (() => {
				const match = thinkingText.match(/^[^\n.!?]*[.!?]?/);
				const raw = match ? match[0].trim() : thinkingText.split("\n")[0];
				return raw.length > 140 ? `${raw.slice(0, 140)}...` : raw;
			})();
			const summaryLabel = t("chat.thinking");

			return (
				<details
					open={isOpen}
					className="chat-aside group my-2 overflow-hidden"
				>
					<summary className="flex items-center gap-2 cursor-pointer select-none px-3 py-2 text-xs text-foreground/70 hover:text-foreground list-none [&::-webkit-details-marker]:hidden [&::marker]:content-['']">
						<svg
							className="h-3.5 w-3.5 shrink-0 transition-transform group-open:rotate-90"
							viewBox="0 0 24 24"
							fill="none"
							aria-hidden="true"
							stroke="currentColor"
							strokeWidth="2"
							strokeLinecap="round"
							strokeLinejoin="round"
						>
							<polyline points="9 18 15 12 9 6" />
						</svg>
						<span className="font-medium">{summaryLabel}</span>
						{firstSentence && (
							<span className="truncate opacity-60 group-open:hidden">
								-- {firstSentence}
							</span>
						)}
					</summary>
					<div
						ref={thinkingCrawlRef}
						className={cn(
							"px-3 pb-3 pt-1 text-xs leading-relaxed",
							isThinkingCommitAnimating &&
								(streamingPresentationMode === "chunked"
									? thinkingAnimationPhase === "a"
										? "stream-commit-slide-in-a"
										: "stream-commit-slide-in-b"
									: undefined),
						)}
						style={
							isStreaming && streamingPresentationMode === "chunked"
								? {
										minHeight: `calc(${thinkingReservedLines} * 1.35em)`,
										contain: "layout paint",
									}
								: undefined
						}
					>
						<MarkdownRenderer
							content={thinkingText}
							className="chat-prose chat-prose--aside overflow-hidden min-w-0 max-w-full"
							isStreaming={isStreaming}
						/>
					</div>
				</details>
			);
		}

		case "tool_call": {
			const hasResult = Boolean(toolResult);
			const toolStatus = hasResult ? "completed" : "running";
			return (
				<ToolCallCard
					part={{
						id: part.id,
						sessionID: "",
						messageID: "",
						type: "tool",
						tool: part.name,
						callID: part.toolCallId,
						state: {
							status: toolStatus,
							input: part.input as Record<string, unknown>,
							output: toolResult
								? formatToolResultOutput(toolResult.output)
								: undefined,
							title: part.name,
						},
					}}
					defaultCollapsed={true}
					hideTodoTools={true}
					collapsible={collapsible}
					hideHeader={hideHeader}
					isRecoveredError={isRecoveredError}
				/>
			);
		}

		case "tool_result":
			// Tool results rendered standalone (no matching tool_call found)
			return (
				<ToolCallCard
					part={{
						id: part.id,
						sessionID: "",
						messageID: "",
						type: "tool",
						tool: part.name || "result",
						callID: part.toolCallId,
						state: {
							status: part.isError ? "error" : "completed",
							output: formatToolResultOutput(part.output),
							title: part.name || "Tool Result",
						},
					}}
					defaultCollapsed={true}
					hideTodoTools={true}
					collapsible={collapsible}
					hideHeader={hideHeader}
					isRecoveredError={isRecoveredError}
				/>
			);

		case "image": {
			const imgPart = part as Extract<DisplayPart, { type: "image" }>;
			let src = "";
			if (imgPart.source === "base64" && "data" in imgPart) {
				src = `data:${imgPart.mimeType ?? "image/png"};base64,${imgPart.data}`;
			} else if (imgPart.source === "url" && "url" in imgPart) {
				src = imgPart.url;
			}
			if (!src) return null;
			return (
				<img
					src={src}
					alt={imgPart.alt ?? "Attached content"}
					className="max-w-[300px] max-h-[300px] rounded-md border border-border object-contain"
					loading="lazy"
				/>
			);
		}

		case "compaction": {
			return (
				<div className="rounded-md border border-border/60 bg-muted/40 px-3 py-2 text-xs text-foreground/70">
					{part.text}
				</div>
			);
		}

		case "error": {
			return (
				<div className="rounded-md border border-destructive/40 bg-destructive/10 px-3 py-2 text-sm text-destructive">
					{part.text}
				</div>
			);
		}

		case "file_ref": {
			const fileRef = part as Extract<DisplayPart, { type: "file_ref" }>;
			const rawUri = (fileRef.uri ?? "").trim();
			const isWebUrl = /^https?:\/\//i.test(rawUri);
			const isDataUrl = /^data:/i.test(rawUri);
			if (isWebUrl || isDataUrl) {
				return (
					<FileReferenceCard
						filePath={fileRef.label ?? rawUri}
						workspacePath={workspacePath}
						directUrl={rawUri}
						label={fileRef.label}
						range={fileRef.range}
					/>
				);
			}

			let candidatePath = rawUri;
			if (candidatePath.startsWith("@")) {
				candidatePath = candidatePath.slice(1);
			}
			if (candidatePath.startsWith("file://")) {
				candidatePath = decodeURIComponent(
					candidatePath.replace("file://", ""),
				);
			}

			if (workspacePath) {
				const root = workspacePath.replace(/\/+$/, "");
				if (candidatePath.startsWith(`${root}/`)) {
					candidatePath = candidatePath.slice(root.length + 1);
				}
			}

			if (candidatePath && !candidatePath.startsWith("/")) {
				return (
					<FileReferenceCard
						filePath={candidatePath}
						workspacePath={workspacePath}
						label={fileRef.label}
						range={fileRef.range}
					/>
				);
			}
			return (
				<div className="rounded-md border border-border/60 px-3 py-2 text-sm text-foreground/80">
					<ExternalLink className="mr-2 inline-block h-4 w-4" />
					{fileRef.label ?? rawUri}
				</div>
			);
		}

		case "audio": {
			const media = part as Extract<Part, { type: "audio" }>;
			let href = "";
			if (media.source === "base64") {
				href = `data:${media.mimeType ?? "audio/mpeg"};base64,${media.data}`;
			} else if (media.source === "url") {
				href = media.url;
			}
			if (!href) return null;
			return (
				<a
					href={href}
					target="_blank"
					rel="noreferrer"
					className="inline-flex items-center gap-2 rounded-md border border-border/60 px-3 py-2 text-sm hover:bg-muted/40"
				>
					<ExternalLink className="h-4 w-4" />
					Audio attachment
				</a>
			);
		}

		case "video": {
			const media = part as Extract<Part, { type: "video" }>;
			let href = "";
			if (media.source === "base64") {
				href = `data:${media.mimeType ?? "video/mp4"};base64,${media.data}`;
			} else if (media.source === "url") {
				href = media.url;
			}
			if (!href) return null;
			return (
				<a
					href={href}
					target="_blank"
					rel="noreferrer"
					className="inline-flex items-center gap-2 rounded-md border border-border/60 px-3 py-2 text-sm hover:bg-muted/40"
				>
					<ExternalLink className="h-4 w-4" />
					Video attachment
				</a>
			);
		}

		case "attachment": {
			const media = part as Extract<Part, { type: "attachment" }>;
			let href = "";
			if (media.source === "base64") {
				href = `data:${media.mimeType ?? "application/octet-stream"};base64,${media.data}`;
			} else if (media.source === "url") {
				href = media.url;
			}
			if (!href) return null;
			return (
				<a
					href={href}
					target="_blank"
					rel="noreferrer"
					className="inline-flex items-center gap-2 rounded-md border border-border/60 px-3 py-2 text-sm hover:bg-muted/40"
				>
					<Paperclip className="h-4 w-4" />
					{media.filename || "Attachment"}
				</a>
			);
		}

		default: {
			if (typeof part.type === "string" && part.type.startsWith("x-")) {
				const extensionPart = part as Extract<Part, { type: `x-${string}` }>;
				return (
					<div className="rounded-md border border-dashed border-border/60 bg-muted/30 p-3 text-xs">
						<div className="mb-1 font-medium text-foreground/80">
							Extension part: {extensionPart.type}
						</div>
						<pre className="whitespace-pre-wrap break-words text-foreground/70">
							{JSON.stringify(
								extensionPart.payload ?? extensionPart.meta ?? null,
								null,
								2,
							)}
						</pre>
					</div>
				);
			}
			console.warn("Unknown Pi message part type:", part);
			return null;
		}
	}
}

function useCoarsePointerDevice(): boolean {
	const [isCoarsePointer, setIsCoarsePointer] = useState(false);

	useEffect(() => {
		const mediaQuery = window.matchMedia("(pointer: coarse)");
		const update = () => setIsCoarsePointer(mediaQuery.matches);
		update();
		mediaQuery.addEventListener("change", update);
		return () => mediaQuery.removeEventListener("change", update);
	}, []);

	return isCoarsePointer;
}

/**
 * Renders text content with @file references as inline previews.
 * Desktop gets a context-menu copy action; touch devices keep native media interactions.
 */
// Estimate reserved lines for live streaming so container growth can lead
// reveal without changing final markdown rendering semantics.

function estimateStreamingReservedLines(text: string): number {
	const normalized = text.trim();
	if (normalized.length === 0) return 2;
	const logicalLines = Math.max(1, normalized.split(/\r?\n/).length);
	const approxCharsPerWrappedLine = 42;
	const wrappedLineEstimate = Math.max(
		1,
		Math.ceil(normalized.length / approxCharsPerWrappedLine),
	);
	return Math.max(2, logicalLines + 1, wrappedLineEstimate + 1);
}

function splitCommittedStreamingTail(
	text: string,
	minPendingChars = 24,
): {
	committed: string;
	tail: string;
} {
	if (text.length === 0) return { committed: "", tail: "" };

	const latestEligibleIdx = Math.max(0, text.length - minPendingChars);
	let splitAt = -1;

	for (let i = latestEligibleIdx; i >= 0; i--) {
		const ch = text.charAt(i);
		if (ch === "\n" || ch === "\r" || /\s|[.,!?;:)}\]]/.test(ch)) {
			splitAt = i + 1;
			break;
		}
	}

	if (splitAt <= 0) {
		return { committed: "", tail: text };
	}

	return {
		committed: text.slice(0, splitAt),
		tail: text.slice(splitAt),
	};
}

function useFrozenStreamingText(content: string, freeze: boolean): string {
	const [frozen, setFrozen] = useState(content);
	// useeffect-guardrail: allow - freeze live streaming text while user inspects scrolled-up history
	useEffect(() => {
		if (!freeze) {
			setFrozen(content);
		}
	}, [content, freeze]);
	return freeze ? frozen : content;
}

function useSmoothContainerHeight(
	ref: { current: HTMLElement | null },
	contentKey: string,
	enabled: boolean,
) {
	const rafRef = useRef<number | null>(null);
	const currentHeightRef = useRef<number | null>(null);
	const targetHeightRef = useRef<number | null>(null);

	// useeffect-guardrail: allow - RAF-driven continuous container height smoothing for smooth streaming mode
	useLayoutEffect(() => {
		void contentKey;
		const el = ref.current;
		if (!el) return;
		const measuredHeight = el.scrollHeight;

		if (!enabled) {
			if (rafRef.current !== null) {
				cancelAnimationFrame(rafRef.current);
				rafRef.current = null;
			}
			currentHeightRef.current = null;
			targetHeightRef.current = null;
			el.style.height = "";
			return;
		}

		targetHeightRef.current = measuredHeight;
		if (currentHeightRef.current === null) {
			currentHeightRef.current = measuredHeight;
			el.style.height = `${measuredHeight}px`;
		}

		if (rafRef.current !== null) return;
		const tick = () => {
			const node = ref.current;
			if (!node) {
				rafRef.current = null;
				return;
			}
			const target = targetHeightRef.current ?? node.scrollHeight;
			const current = currentHeightRef.current ?? target;
			const next = current + (target - current) * 0.22;
			if (Math.abs(target - next) <= 0.25) {
				node.style.height = `${target}px`;
				currentHeightRef.current = target;
				rafRef.current = null;
				return;
			}
			node.style.height = `${next}px`;
			currentHeightRef.current = next;
			rafRef.current = requestAnimationFrame(tick);
		};
		rafRef.current = requestAnimationFrame(tick);
	}, [contentKey, enabled, ref]);

	// useeffect-guardrail: allow - cleanup height smoothing RAF and inline styles on unmount/ref changes
	useEffect(() => {
		const el = ref.current;
		return () => {
			if (rafRef.current !== null) {
				cancelAnimationFrame(rafRef.current);
				rafRef.current = null;
			}
			currentHeightRef.current = null;
			targetHeightRef.current = null;
			if (!el) return;
			el.style.height = "";
		};
	}, [ref]);
}

function useSmoothCrawlTransform(
	ref: { current: HTMLElement | null },
	contentKey: string,
	enabled: boolean,
) {
	const previousHeightRef = useRef<number | null>(null);
	const offsetRef = useRef(0);
	const animRafRef = useRef<number | null>(null);

	// useeffect-guardrail: allow - RAF-driven continuous crawl transform for streaming content
	useLayoutEffect(() => {
		void contentKey;
		const el = ref.current;
		if (!el) return;

		const nextHeight = el.scrollHeight;
		const prevHeight = previousHeightRef.current ?? nextHeight;
		previousHeightRef.current = nextHeight;

		if (!enabled) {
			if (animRafRef.current !== null) {
				cancelAnimationFrame(animRafRef.current);
				animRafRef.current = null;
			}
			offsetRef.current = 0;
			el.style.willChange = "";
			el.style.transform = "";
			return;
		}

		const delta = nextHeight - prevHeight;
		if (delta > 1.25) {
			offsetRef.current = Math.min(42, offsetRef.current + delta);
		}

		if (animRafRef.current !== null) {
			return;
		}

		const tick = () => {
			const node = ref.current;
			if (!node) {
				animRafRef.current = null;
				return;
			}
			const current = offsetRef.current;
			if (current <= 0.08) {
				offsetRef.current = 0;
				node.style.transform = "translateY(0px)";
				node.style.willChange = "";
				animRafRef.current = null;
				return;
			}

			node.style.willChange = "transform";
			node.style.transform = `translateY(${current.toFixed(3)}px)`;
			const decayed = current * 0.82;
			offsetRef.current = decayed < 0.08 ? 0 : decayed;
			animRafRef.current = requestAnimationFrame(tick);
		};

		animRafRef.current = requestAnimationFrame(tick);
	}, [contentKey, enabled, ref]);

	// useeffect-guardrail: allow - cleanup RAF and inline transform styles on unmount/ref changes
	useEffect(() => {
		const el = ref.current;
		return () => {
			if (animRafRef.current !== null) {
				cancelAnimationFrame(animRafRef.current);
				animRafRef.current = null;
			}
			offsetRef.current = 0;
			if (!el) return;
			el.style.willChange = "";
			el.style.transform = "";
		};
	}, [ref]);
}

function useStreamingCommittedContent(
	content: string,
	isStreaming: boolean,
	mode: StreamingPresentationMode,
): {
	visibleContent: string;
	isCommitAnimating: boolean;
	animationPhase: "a" | "b";
} {
	const [isCommitAnimating, setIsCommitAnimating] = useState(false);
	const [animationPhase, setAnimationPhase] = useState<"a" | "b">("a");
	const [isResizeSettling, setIsResizeSettling] = useState(false);
	const resizeSettleTimeoutRef = useRef<number | null>(null);
	const animateTimeoutRef = useRef<number | null>(null);
	const previousCommittedRef = useRef("");

	const split = useMemo(() => {
		if (!isStreaming) return { committed: content, tail: "" };
		if (mode !== "chunked") return { committed: content, tail: "" };
		return splitCommittedStreamingTail(content, 24);
	}, [content, isStreaming, mode]);

	const visibleContent = isStreaming ? split.committed : content;

	// useeffect-guardrail: allow - resize debounce gate for streaming animations
	useEffect(() => {
		if (!isStreaming) {
			setIsResizeSettling(false);
			return;
		}
		const onResize = () => {
			setIsResizeSettling(true);
			if (resizeSettleTimeoutRef.current !== null) {
				window.clearTimeout(resizeSettleTimeoutRef.current);
			}
			resizeSettleTimeoutRef.current = window.setTimeout(() => {
				setIsResizeSettling(false);
				resizeSettleTimeoutRef.current = null;
			}, 220);
		};
		window.addEventListener("resize", onResize, { passive: true });
		return () => {
			window.removeEventListener("resize", onResize);
			if (resizeSettleTimeoutRef.current !== null) {
				window.clearTimeout(resizeSettleTimeoutRef.current);
				resizeSettleTimeoutRef.current = null;
			}
		};
	}, [isStreaming]);

	// useeffect-guardrail: allow - animate newly committed streaming chunks
	useEffect(() => {
		if (!isStreaming) {
			previousCommittedRef.current = visibleContent;
			setIsCommitAnimating(false);
			if (animateTimeoutRef.current !== null) {
				window.clearTimeout(animateTimeoutRef.current);
				animateTimeoutRef.current = null;
			}
			return;
		}

		const previous = previousCommittedRef.current;
		const next = visibleContent;
		const hasNewCommittedContent = next.length > previous.length;
		previousCommittedRef.current = next;

		if (!hasNewCommittedContent || isResizeSettling || mode === "raw") {
			return;
		}

		setAnimationPhase((prev) => (prev === "a" ? "b" : "a"));
		setIsCommitAnimating(true);
		if (animateTimeoutRef.current !== null) {
			window.clearTimeout(animateTimeoutRef.current);
		}
		animateTimeoutRef.current = window.setTimeout(() => {
			setIsCommitAnimating(false);
			animateTimeoutRef.current = null;
		}, 140);
		return () => {
			if (animateTimeoutRef.current !== null) {
				window.clearTimeout(animateTimeoutRef.current);
				animateTimeoutRef.current = null;
			}
		};
	}, [isResizeSettling, isStreaming, mode, visibleContent]);

	return {
		visibleContent,
		isCommitAnimating: isCommitAnimating && !isResizeSettling,
		animationPhase,
	};
}

function TextWithFileReferences({
	content,
	workspacePath,
	locale = "en",
	onFileReferenceOpen,
	onImageOpen,
	deferMermaidUntilFinal = false,
	isStreaming = false,
	freezeStreamingUpdates = false,
	streamingPresentationMode = "raw",
}: {
	content: string;
	workspacePath?: string | null;
	locale?: "en" | "de";
	onFileReferenceOpen?: FileReferenceOpenHandler;
	onImageOpen?: ImageOpenHandler;
	deferMermaidUntilFinal?: boolean;
	isStreaming?: boolean;
	freezeStreamingUpdates?: boolean;
	streamingPresentationMode?: StreamingPresentationMode;
}) {
	const { t } = useTranslation();
	const fileAdapter = useContext(ChatFileAdapterContext);
	const smoothCrawlRef = useRef<HTMLDivElement>(null);
	const frozenContent = useFrozenStreamingText(
		content,
		freezeStreamingUpdates && isStreaming,
	);
	const { visibleContent, isCommitAnimating, animationPhase } =
		useStreamingCommittedContent(
			frozenContent,
			isStreaming,
			streamingPresentationMode,
		);
	useSmoothContainerHeight(
		smoothCrawlRef,
		visibleContent,
		isStreaming &&
			streamingPresentationMode === "smooth" &&
			!freezeStreamingUpdates,
	);
	useSmoothCrawlTransform(
		smoothCrawlRef,
		visibleContent,
		isStreaming &&
			streamingPresentationMode === "smooth" &&
			!freezeStreamingUpdates,
	);
	// Strip ANSI escape codes and fix indentation before rendering.
	// Some models (e.g. Kimi-K2.5) prefix text with 4+ spaces which CommonMark
	// interprets as indented code blocks, causing plain text to render as <pre><code>.
	const cleanContent = dedentMarkdown(stripAnsiSequences(visibleContent));
	const reservationBasis = dedentMarkdown(stripAnsiSequences(frozenContent));

	// Rewrite markdown image URLs that reference local workspace files
	// (e.g. ![avatar](ginee_pixel_art_avatar.png)) so they resolve through
	// the authenticated workspace file endpoint.
	const markdownContent = useMemo(() => {
		if (!workspacePath) return cleanContent;
		return cleanContent.replace(
			/!\[([^\]]*)\]\(([^)]+)\)/g,
			(_m, alt, rawTarget) => {
				let target = String(rawTarget).trim();
				if (!target) return _m;
				if (target.startsWith("<") && target.endsWith(">")) {
					target = target.slice(1, -1).trim();
				}
				// Ignore absolute/data/blob/anchor URLs
				if (
					/^(https?:|data:|blob:|#)/i.test(target) ||
					target.startsWith("/api/") ||
					target.startsWith("//")
				) {
					return _m;
				}
				if (!fileAdapter) return _m;
				const url = fileAdapter.fileUrl(workspacePath, target);
				return `![${alt}](${url})`;
			},
		);
	}, [cleanContent, fileAdapter, workspacePath]);

	// Parse @file references, excluding code blocks
	const fileRefs = useMemo(
		() => extractFileReferenceDetails(cleanContent),
		[cleanContent],
	);

	// The attachment manifest is stripped only when its files are actually
	// shown somewhere else, which is the same condition the chips render on.
	const readableContent = useMemo(
		() =>
			fileRefs.length > 0 && workspacePath
				? withoutMentionOnlyLines(markdownContent)
				: markdownContent,
		[fileRefs.length, markdownContent, workspacePath],
	);

	// An attachment is shown once, as a chip. Media keeps its preview card,
	// because a thumbnail is the only honest preview of an image.
	const { mediaRefs, chipRefs } = useMemo(() => {
		const media: typeof fileRefs = [];
		const chips: typeof fileRefs = [];
		for (const ref of fileRefs) {
			const category = getFileTypeInfo(ref.filePath).category;
			(category === "image" || category === "video" ? media : chips).push(ref);
		}
		return { mediaRefs: media, chipRefs: chips };
	}, [fileRefs]);
	const streamingReservedLines = useMemo(
		() =>
			isStreaming && streamingPresentationMode === "chunked"
				? estimateStreamingReservedLines(reservationBasis)
				: 0,
		[isStreaming, reservationBasis, streamingPresentationMode],
	);
	const isCoarsePointerDevice = useCoarsePointerDevice();

	const contentBlock = (
		<div
			ref={smoothCrawlRef}
			className={cn(
				"space-y-2 select-none sm:select-auto min-w-0 max-w-full",
				isCommitAnimating &&
					(streamingPresentationMode === "chunked"
						? animationPhase === "a"
							? "stream-commit-slide-in-a"
							: "stream-commit-slide-in-b"
						: undefined),
			)}
			style={
				isStreaming && streamingPresentationMode === "chunked"
					? {
							minHeight: `calc(${streamingReservedLines} * 1.55em)`,
							contain: "layout paint",
						}
					: undefined
			}
		>
			<MarkdownRenderer
				content={readableContent}
				className="canonical-message-prose chat-prose overflow-hidden min-w-0 max-w-full"
				enableMermaid={!deferMermaidUntilFinal}
				isStreaming={isStreaming}
				onFileReferenceOpen={(reference) =>
					onFileReferenceOpen?.(reference.filePath, {
						startLine: reference.startLine,
						endLine: reference.endLine,
					})
				}
				onImageOpen={onImageOpen}
			/>
			{chipRefs.length > 0 && workspacePath && (
				<div className="chat-chips">
					{chipRefs.map((ref) => (
						<button
							key={`${ref.filePath}-${ref.label}`}
							type="button"
							className="chat-chip"
							title={ref.filePath}
							onClick={() =>
								onFileReferenceOpen?.(ref.filePath, {
									startLine: ref.startLine,
									endLine: ref.endLine,
								})
							}
						>
							<FileText aria-hidden="true" />
							<span className="chat-truncate">{ref.label}</span>
						</button>
					))}
				</div>
			)}
			{mediaRefs.length > 0 && workspacePath && (
				<div className="flex flex-wrap gap-2 mt-2">
					{mediaRefs.map((ref) => (
						<FileReferenceCard
							key={`${ref.filePath}-${ref.label}`}
							filePath={ref.filePath}
							workspacePath={workspacePath}
							label={ref.label}
							onOpenFileReference={onFileReferenceOpen}
						/>
					))}
				</div>
			)}
		</div>
	);

	if (isCoarsePointerDevice) {
		return contentBlock;
	}

	return (
		<ContextMenu>
			<ContextMenuTrigger className="contents">
				{contentBlock}
			</ContextMenuTrigger>
			<ContextMenuContent>
				<ContextMenuItem
					onClick={() => navigator.clipboard?.writeText(cleanContent)}
					className="gap-2"
				>
					<Copy className="w-4 h-4" />
					{t("chat.copy")}
				</ContextMenuItem>
			</ContextMenuContent>
		</ContextMenu>
	);
}

/**
 * Passive card for an explicit file reference. Rendering never probes or reads
 * the file; content access begins only after the user activates the preview.
 */
export const FileReferenceCard = memo(function FileReferenceCard({
	filePath,
	workspacePath,
	directUrl,
	label,
	onOpenFileReference,
	fileAdapter: explicitFileAdapter,
	range,
}: {
	filePath: string;
	workspacePath?: string | null;
	directUrl?: string;
	label?: string;
	onOpenFileReference?: FileReferenceOpenHandler;
	fileAdapter?: ChatFileAdapter | null;
	range?: FileRange;
}) {
	const inheritedFileAdapter = useContext(ChatFileAdapterContext);
	const inheritedOpenFileReference = useContext(FileReferenceOpenContext);
	const fileAdapter = explicitFileAdapter ?? inheritedFileAdapter;
	const openFileReference = onOpenFileReference ?? inheritedOpenFileReference;
	const [isLoading, setIsLoading] = useState(true);
	const [error, setError] = useState<string | null>(null);
	const [imageLoaded, setImageLoaded] = useState(false);
	const [fileExists, setFileExists] = useState<boolean | null>(null);
	const [fileUrl, setFileUrl] = useState<string | null>(null);
	const [resolvedWorkspacePath, setResolvedWorkspacePath] =
		useState<string>("");

	const workspaceScopedPath = useMemo(() => {
		const workspaceRoot = (workspacePath ?? "").replace(/\/+$/, "");
		if (!workspaceRoot) return filePath;
		if (filePath === workspaceRoot) return ".";
		if (filePath.startsWith(`${workspaceRoot}/`)) {
			return filePath.slice(workspaceRoot.length + 1);
		}
		return filePath;
	}, [filePath, workspacePath]);

	const candidateWorkspacePaths = useMemo(() => {
		const candidates = new Set<string>([workspaceScopedPath]);
		if (workspaceScopedPath.includes("%")) {
			try {
				candidates.add(decodeURIComponent(workspaceScopedPath));
			} catch {
				// ignore malformed encoding
			}
		}
		return [...candidates].filter((v) => v.length > 0);
	}, [workspaceScopedPath]);

	const fileInfo = useMemo(
		() => getFileTypeInfo(workspaceScopedPath),
		[workspaceScopedPath],
	);
	const isImage = fileInfo.category === "image";
	const isVideo = fileInfo.category === "video";
	const fileName =
		label || workspaceScopedPath.split("/").pop() || workspaceScopedPath;

	// Download file
	const handleDownload = useCallback(async () => {
		try {
			if (directUrl) {
				window.open(directUrl, "_blank", "noopener");
				return;
			}
			if (!workspacePath) return;
			await fileAdapter?.downloadFile?.(
				workspacePath,
				resolvedWorkspacePath || workspaceScopedPath,
				fileName,
			);
		} catch (err) {
			console.error("Failed to download file:", err);
		}
	}, [
		directUrl,
		fileAdapter,
		workspacePath,
		resolvedWorkspacePath,
		workspaceScopedPath,
		fileName,
	]);

	// Open in canvas (only for images)
	const handleOpenInCanvas = useCallback(() => {
		if (!isImage) return;
		// Dispatch custom event that SessionScreen can listen to
		window.dispatchEvent(
			new CustomEvent("oqto:open-in-canvas", {
				detail: {
					imagePath: resolvedWorkspacePath || workspaceScopedPath,
				},
			}),
		);
	}, [isImage, resolvedWorkspacePath, workspaceScopedPath]);

	useEffect(() => {
		let cancelled = false;
		setIsLoading(true);
		setError(null);
		setFileExists(null);
		setImageLoaded(false);
		setFileUrl(directUrl ?? null);
		setResolvedWorkspacePath("");

		if (!workspacePath && !directUrl) {
			setFileExists(false);
			setIsLoading(false);
			return;
		}

		const run = async () => {
			const resolvedPath =
				candidateWorkspacePaths.find((candidate) => !candidate.includes("%")) ??
				candidateWorkspacePaths[0] ??
				workspaceScopedPath;
			if (cancelled) return;
			setResolvedWorkspacePath(resolvedPath);
			setFileExists(true);

			if ((isImage || isVideo) && !directUrl && workspacePath) {
				setFileUrl(fileAdapter?.fileUrl(workspacePath, resolvedPath) ?? null);
			}
			if (!isImage && !isVideo) {
				setIsLoading(false);
			}
		};

		void run();
		return () => {
			cancelled = true;
		};
	}, [
		candidateWorkspacePaths,
		directUrl,
		fileAdapter,
		isImage,
		isVideo,
		workspacePath,
		workspaceScopedPath,
	]);

	// Resolve card presentation without probing the workspace.
	if (fileExists === null) {
		return (
			<div className="inline-flex items-center gap-2 px-3 py-1.5 border border-border bg-muted/20 rounded text-xs text-muted-foreground">
				<Loader2 className="w-3.5 h-3.5 animate-spin" />
				Loading preview…
			</div>
		);
	}

	if (fileExists === false || ((isImage || isVideo) && !fileUrl)) {
		const FileIcon = fileInfo.category === "code" ? FileCode : FileText;
		return (
			<button
				type="button"
				onClick={() => {
					if (directUrl) {
						window.open(directUrl, "_blank", "noopener");
						return;
					}
					if (!workspacePath) return;
					void fileAdapter?.downloadFile?.(
						workspacePath,
						resolvedWorkspacePath || workspaceScopedPath,
						fileName,
					);
				}}
				className="inline-flex items-center gap-2 px-3 py-1.5 border border-border bg-muted/20 rounded hover:bg-muted/40 transition-colors text-sm"
			>
				<FileIcon className="w-4 h-4 text-muted-foreground" />
				<span className="font-medium">{fileName}</span>
				<span className="text-xs text-muted-foreground">
					Preview unavailable
				</span>
				<ExternalLink className="w-3 h-3 text-muted-foreground" />
			</button>
		);
	}

	// For images, render inline preview
	if (isImage) {
		return (
			<div className="border border-border bg-muted/20 rounded overflow-hidden max-w-md">
				<div className="flex items-center justify-between px-3 py-2 bg-muted/50 border-b border-border">
					<div className="flex items-center gap-2 min-w-0">
						<FileImage className="w-4 h-4 text-muted-foreground shrink-0" />
						<span className="text-xs font-medium truncate">{fileName}</span>
					</div>
					<div className="flex items-center gap-1 shrink-0">
						<button
							type="button"
							onClick={handleOpenInCanvas}
							className="p-1.5 text-muted-foreground hover:text-foreground hover:bg-muted/80 rounded transition-colors"
							title="Open in canvas"
						>
							<PaintBucket className="w-3.5 h-3.5" />
						</button>
						<button
							type="button"
							onClick={handleDownload}
							className="p-1.5 text-muted-foreground hover:text-foreground hover:bg-muted/80 rounded transition-colors"
							title="Download"
						>
							<Download className="w-3.5 h-3.5" />
						</button>
					</div>
				</div>
				<div className="relative">
					{isLoading && !imageLoaded && (
						<div className="flex items-center justify-center p-4">
							<Loader2 className="w-5 h-5 animate-spin text-muted-foreground" />
						</div>
					)}
					{error ? (
						<div className="flex items-center justify-center p-4 text-xs text-muted-foreground">
							{error}
						</div>
					) : (
						<img
							src={fileUrl ?? ""}
							alt={fileName}
							className={cn(
								"max-w-full h-auto",
								isLoading && !imageLoaded && "hidden",
							)}
							onLoad={() => {
								setImageLoaded(true);
								setIsLoading(false);
							}}
							onError={() => {
								setError("Failed to load image");
								setIsLoading(false);
							}}
						/>
					)}
				</div>
			</div>
		);
	}

	// Video preview
	if (isVideo) {
		return (
			<div className="border border-border bg-muted/20 rounded overflow-hidden max-w-md">
				<div className="flex items-center justify-between px-3 py-2 bg-muted/50 border-b border-border">
					<div className="flex items-center gap-2 min-w-0">
						<FileVideo className="w-4 h-4 text-muted-foreground shrink-0" />
						<span className="text-xs font-medium truncate">{fileName}</span>
					</div>
					<button
						type="button"
						onClick={handleDownload}
						className="p-1.5 text-muted-foreground hover:text-foreground hover:bg-muted/80 rounded transition-colors shrink-0"
						title="Download"
					>
						<Download className="w-3.5 h-3.5" />
					</button>
				</div>
				<video
					src={fileUrl ?? ""}
					controls
					playsInline
					className="max-w-full h-auto"
					onLoadedData={() => setIsLoading(false)}
					onError={() => {
						setError("Failed to load video");
						setIsLoading(false);
					}}
				>
					<track kind="captions" />
					Your browser does not support the video tag.
				</video>
			</div>
		);
	}

	// For non-images/videos, render a compact file reference link
	const FileIcon = fileInfo.category === "code" ? FileCode : FileText;
	return (
		<button
			type="button"
			onClick={() => {
				if (directUrl) {
					window.open(directUrl, "_blank", "noopener");
					return;
				}
				if (openFileReference) {
					openFileReference(workspaceScopedPath, range);
					return;
				}
				if (!workspacePath) return;
				void fileAdapter?.downloadFile?.(
					workspacePath,
					workspaceScopedPath,
					fileName,
				);
			}}
			className="group flex w-full max-w-md min-w-0 items-center gap-2.5 border border-border bg-muted/20 px-3 py-2 text-left text-sm transition-colors hover:bg-muted/40"
			data-testid="file-reference-card"
		>
			<FileIcon className="h-4 w-4 shrink-0 text-muted-foreground" />
			<span className="min-w-0 flex-1">
				<span className="block truncate font-medium" title={fileName}>
					{fileName}
				</span>
				{workspaceScopedPath !== fileName && (
					<span
						className="block truncate text-xs text-muted-foreground"
						title={workspaceScopedPath}
					>
						{workspaceScopedPath}
					</span>
				)}
			</span>
			<ExternalLink className="h-3.5 w-3.5 shrink-0 text-muted-foreground transition-colors group-hover:text-foreground" />
		</button>
	);
});
