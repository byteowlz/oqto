import { A2UICallCard } from "@/components/chat/a2ui-call-card";
import { ToolCallCard, getToolIcon } from "@/components/chat/tool-call-card";
import { ToolCallGroup } from "@/components/chat/tool-call-group";
import { BrailleSpinner } from "@/components/common";
import { CopyButton, MarkdownRenderer } from "@/components/data-display";
import {
	ContextMenu,
	ContextMenuContent,
	ContextMenuItem,
	ContextMenuTrigger,
} from "@/components/ui/context-menu";
import {
	Popover,
	PopoverContent,
	PopoverTrigger,
} from "@/components/ui/popover";
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
	Bot,
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
	User,
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

// Compact copy button for message headers
function CompactCopyButton({
	text,
	className,
}: { text: string; className?: string }) {
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
			className={cn("text-muted-foreground hover:text-foreground", className)}
		>
			{copied ? (
				<Check className="w-3 h-3 text-primary" />
			) : (
				<Copy className="w-3 h-3" />
			)}
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
function splitToolGroupIntoRuns(toolGroup: {
	key: string;
	type: "tool_group";
	segments: Array<
		| Extract<Segment, { type: "tool_call" }>
		| Extract<Segment, { type: "tool_result_only" }>
	>;
	timestamp: number;
}) {
	const runs: Array<{
		key: string;
		type: "tool_group";
		segments: Array<
			| Extract<Segment, { type: "tool_call" }>
			| Extract<Segment, { type: "tool_result_only" }>
		>;
		timestamp: number;
	}> = [];

	let currentRun: Array<
		| Extract<Segment, { type: "tool_call" }>
		| Extract<Segment, { type: "tool_result_only" }>
	> = [];
	let currentName: string | null = null;

	for (const seg of toolGroup.segments) {
		const name =
			seg.type === "tool_call" ? seg.part.name : seg.part.name || "result";
		if (currentRun.length === 0 || name === currentName) {
			currentRun.push(seg);
			currentName = name;
			continue;
		}
		runs.push({
			key: `${toolGroup.key}-run-${runs.length}`,
			type: "tool_group",
			segments: currentRun,
			timestamp: currentRun[0]?.timestamp ?? toolGroup.timestamp,
		});
		currentRun = [seg];
		currentName = name;
	}

	if (currentRun.length > 0) {
		runs.push({
			key: `${toolGroup.key}-run-${runs.length}`,
			type: "tool_group",
			segments: currentRun,
			timestamp: currentRun[0]?.timestamp ?? toolGroup.timestamp,
		});
	}

	return runs;
}

function ToolGutterIcon({
	toolGroup,
	locale,
}: {
	toolGroup: {
		key: string;
		type: "tool_group";
		segments: Array<
			| Extract<Segment, { type: "tool_call" }>
			| Extract<Segment, { type: "tool_result_only" }>
		>;
		timestamp: number;
	};
	locale: "en" | "de";
}) {
	// Collapse consecutive identical tools
	const collapsed: Array<{
		toolName: string;
		count: number;
		input?: Record<string, unknown>;
	}> = [];
	for (const seg of toolGroup.segments) {
		const toolName =
			seg.type === "tool_call" ? seg.part.name : seg.part.name || "result";
		const input =
			seg.type === "tool_call"
				? (seg.part.input as Record<string, unknown> | undefined)
				: undefined;
		const last = collapsed[collapsed.length - 1];
		if (last && last.toolName === toolName) {
			last.count += 1;
			continue;
		}
		collapsed.push({ toolName, count: 1, input });
	}

	// Pick the most representative icon (first entry)
	const primaryIcon = getToolIcon(
		collapsed[0]?.toolName ?? "tool",
		collapsed[0]?.input,
	);
	const totalCount = toolGroup.segments.length;

	return (
		<Popover>
			<PopoverTrigger asChild>
				<button
					type="button"
					className="relative flex h-6 w-6 items-center justify-center rounded hover:bg-muted transition-colors text-muted-foreground/60 hover:text-muted-foreground"
					title={`${totalCount} tool call${totalCount !== 1 ? "s" : ""}`}
				>
					{primaryIcon}
					{totalCount > 1 && (
						<span className="absolute top-0 right-0 min-w-[12px] h-[12px] bg-muted-foreground/20 text-muted-foreground text-[8px] rounded-full inline-flex items-center justify-center leading-none">
							{totalCount}
						</span>
					)}
				</button>
			</PopoverTrigger>
			<PopoverContent
				side="left"
				align="start"
				className="w-64 max-w-[calc(100vw-2rem)] p-2"
			>
				<div className="space-y-1">
					{collapsed.map((entry, i) => {
						const icon = getToolIcon(entry.toolName, entry.input);
						const summary = getToolSummary(entry.toolName, entry.input, locale);
						const label = summary ?? entry.toolName;
						return (
							<div
								key={`${entry.toolName}-${i}`}
								className="flex min-w-0 items-start gap-2 rounded px-1.5 py-1 text-xs text-muted-foreground"
							>
								<span className="mt-0.5 shrink-0">{icon}</span>
								<span className="min-w-0 flex-1 leading-snug break-words [overflow-wrap:anywhere]">
									{label}
									{entry.count > 1 && (
										<span className="ml-1 opacity-60">x{entry.count}</span>
									)}
								</span>
							</div>
						);
					})}
				</div>
			</PopoverContent>
		</Popover>
	);
}

export type ChatFileAdapter = {
	fileUrl: (workspacePath: string, filePath: string) => string;
	renderReadAloud?: (text: string) => ReactNode;
	downloadFile?: (
		workspacePath: string,
		filePath: string,
		fileName: string,
	) => Promise<void>;
};

const ChatFileAdapterContext = createContext<ChatFileAdapter | null>(null);
type FileReferenceOpenHandler = (filePath: string, range?: FileRange) => void;
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

	const renderSegments: RenderSegment[] = (() => {
		if (verbosity === 1) {
			const grouped: RenderSegment[] = [];
			let toolBuffer: Extract<
				RenderSegment,
				{ type: "tool_group" }
			>["segments"] = [];
			let thinkingBuffer: string[] = [];
			let thinkingKey: string | null = null;
			let thinkingTimestamp = 0;

			const flushThinkingOnly = () => {
				if (thinkingBuffer.length > 0) {
					grouped.push({
						key: thinkingKey ?? `thinking-${grouped.length}`,
						type: "thinking",
						text: thinkingBuffer.join("\n\n"),
						timestamp: thinkingTimestamp,
					});
					thinkingBuffer = [];
					thinkingKey = null;
					thinkingTimestamp = 0;
				}
			};

			// Tool group waiting to be attached to the next content segment.
			let pendingToolGroup: {
				key: string;
				type: "tool_group";
				segments: typeof toolBuffer;
				timestamp: number;
			} | null = null;

			const attachToolGroup = () => {
				if (toolBuffer.length === 0) return;
				const tg = {
					key: `tool-group-${toolBuffer[0].key}`,
					type: "tool_group" as const,
					segments: toolBuffer,
					timestamp: toolBuffer[0].timestamp,
				};
				toolBuffer = [];
				// Attach to the last grouped segment (text or thinking)
				const last = grouped[grouped.length - 1];
				if (last) {
					(last as RenderSegment & { _toolGroup?: typeof tg })._toolGroup = tg;
				} else {
					// No preceding segment -- attach to the next one
					pendingToolGroup = tg;
				}
			};

			const pushSegment = (seg: RenderSegment) => {
				// Attach any pending (leading) tool group to this segment
				if (pendingToolGroup) {
					(
						seg as RenderSegment & { _toolGroup?: typeof pendingToolGroup }
					)._toolGroup = pendingToolGroup;
					pendingToolGroup = null;
				}
				grouped.push(seg);
			};

			for (const segment of displaySegments) {
				if (segment.type === "thinking") {
					if (thinkingBuffer.length === 0) {
						thinkingKey = segment.key;
						thinkingTimestamp = segment.timestamp;
					}
					thinkingBuffer.push(segment.text);
					continue;
				}
				if (
					segment.type === "tool_call" ||
					segment.type === "tool_result_only"
				) {
					toolBuffer.push(segment);
					continue;
				}
				// Non-thinking, non-tool segment: flush pending items
				flushThinkingOnly();
				attachToolGroup();
				pushSegment(segment);
			}
			flushThinkingOnly();
			attachToolGroup();
			// If only tool calls remain (no text/thinking segments to attach to),
			// emit the pending tool group as a standalone segment so the message
			// bubble isn't empty.
			if (pendingToolGroup && grouped.length === 0) {
				grouped.push(pendingToolGroup);
				pendingToolGroup = null;
			}
			return grouped;
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
				"group transition-colors duration-200 overflow-hidden min-w-0 max-w-full",
				isUser
					? "sm:ml-8 bg-primary/20 dark:bg-primary/10 border border-primary/40 dark:border-primary/30"
					: "sm:mr-8 bg-muted/50 border border-border",
			)}
		>
			<div
				className={cn(
					"compact-header flex items-center gap-1 sm:gap-2 px-2 sm:px-3 py-1.5 sm:py-2 border-b overflow-hidden",
					isUser ? "border-primary/30 dark:border-primary/20" : "border-border",
				)}
			>
				{isUser ? (
					<User className="w-3 h-3 sm:w-4 sm:h-4 text-primary flex-shrink-0" />
				) : (
					<Bot className="w-3 h-3 sm:w-4 sm:h-4 text-primary flex-shrink-0" />
				)}
				{isUser ? (
					<span className="text-sm font-medium text-foreground truncate min-w-0">
						{group.messages[0]?.sender?.name ?? t("chat.you")}
					</span>
				) : (
					<span className="text-sm font-medium text-foreground truncate min-w-0">
						{assistantDisplayName}
					</span>
				)}
				{group.messages.length > 1 && (
					<span
						className={cn(
							"inline-flex min-h-4 items-center justify-center rounded-sm border px-1 text-[9px] sm:text-[10px] leading-tight flex-shrink-0",
							isUser
								? "border-primary/30 text-primary"
								: "border-border text-muted-foreground",
						)}
					>
						{group.messages.length}
					</span>
				)}
				<div className="flex-1" />
				{!isUser && allTextContent
					? fileAdapter?.renderReadAloud?.(allTextContent)
					: null}
				{isUser && onForkHere && (
					<button
						type="button"
						onClick={(e) => {
							e.stopPropagation();
							onForkHere();
						}}
						className="text-muted-foreground hover:text-foreground transition-colors"
						title={t("chat.forkHere", "Fork here")}
					>
						<GitBranch className="w-3.5 h-3.5" />
					</button>
				)}
				{createdAt && !Number.isNaN(createdAt.getTime()) && (
					<span className="text-[9px] sm:text-[10px] text-foreground/50 dark:text-muted-foreground leading-none sm:leading-normal ml-2 flex-shrink-0">
						{createdAt.toLocaleTimeString([], {
							hour: "2-digit",
							minute: "2-digit",
						})}
					</span>
				)}
				{allTextContent && (
					<CopyButton
						text={allTextContent}
						className="hidden sm:inline-flex ml-1 [&_svg]:w-3 [&_svg]:h-3"
					/>
				)}
				{allTextContent && (
					<CompactCopyButton text={allTextContent} className="sm:hidden ml-1" />
				)}
			</div>

			<div
				className={cn(
					"relative px-2 sm:px-4 py-2 sm:py-3 group space-y-2 overflow-hidden min-w-0 max-w-full transition-[padding] duration-150 ease-out",
					!isUser && verbosity === 1 && "pr-12 sm:pr-16",
				)}
			>
				{!isUser && verbosity === 1 && (
					<div className="absolute top-2 bottom-2 right-[2.25rem] sm:right-[2.75rem] w-px bg-border/40" />
				)}
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

					// In minimal mode, segments may have an attached _toolGroup.
					// Wrap content + gutter icon in a flex row.
					const attachedToolGroup = (
						segment as RenderSegment & {
							_toolGroup?: Extract<RenderSegment, { type: "tool_group" }>;
						}
					)._toolGroup;

					const wrapWithGutter = (
						key: string,
						content: React.ReactNode,
						isThinking = false,
					) => {
						if (!attachedToolGroup || verbosity !== 1) {
							return content;
						}
						const runs = splitToolGroupIntoRuns(attachedToolGroup);
						return (
							<div
								key={key}
								className={cn(
									"relative",
									isThinking &&
										"[&>.tool-gutter]:hidden [&:has(details[open])>.tool-gutter]:flex",
								)}
							>
								{content}
								<div
									className={cn(
										"absolute right-[-2.625rem] sm:right-[-3.375rem] top-0 bottom-0 items-center",
										isThinking ? "tool-gutter" : "flex",
									)}
								>
									<div className="flex flex-col items-center gap-1 py-0.5">
										{runs.map((run) => (
											<ToolGutterIcon
												key={run.key}
												toolGroup={run}
												locale={locale}
											/>
										))}
									</div>
								</div>
							</div>
						);
					};

					if (segment.type === "text") {
						const inner = (
							<TextWithFileReferences
								key={segment.key}
								content={segment.text}
								workspacePath={workspacePath}
								locale={locale}
								onFileReferenceOpen={onFileReferenceOpen}
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
						return wrapWithGutter(segment.key, inner);
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
						return wrapWithGutter(segment.key, inner, true);
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
						// In minimal mode, tool groups are usually attached to neighboring
						// text/thinking segments via _toolGroup. But tool-only assistant
						// messages can produce a standalone tool_group. Render a compact
						// fallback row so the bubble is never empty.
						if (verbosity === 1) {
							return (
								<div
									key={segment.key}
									className={cn(
										"relative h-6 leading-none text-xs text-muted-foreground",
										needsTopMargin && "mt-2",
									)}
								>
									<span>{t("chat.toolsUsed", "Used tools")}</span>
									<div className="absolute right-[-2.625rem] sm:right-[-3.375rem] top-0 bottom-0 flex items-center">
										<ToolGutterIcon toolGroup={segment} locale={locale} />
									</div>
								</div>
							);
						}
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
					className="group my-2 border-l-2 border-primary/40 bg-muted/40 pl-0 overflow-hidden"
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
							className="text-xs text-foreground/70 leading-relaxed overflow-hidden min-w-0 max-w-full [&_p]:text-foreground/70 [&_li]:text-foreground/70 [&_code]:text-foreground/60"
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
	deferMermaidUntilFinal = false,
	isStreaming = false,
	freezeStreamingUpdates = false,
	streamingPresentationMode = "raw",
}: {
	content: string;
	workspacePath?: string | null;
	locale?: "en" | "de";
	onFileReferenceOpen?: FileReferenceOpenHandler;
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
				content={markdownContent}
				className="text-sm text-foreground leading-relaxed overflow-hidden min-w-0 max-w-full"
				enableMermaid={!deferMermaidUntilFinal}
				isStreaming={isStreaming}
				onFileReferenceOpen={(reference) =>
					onFileReferenceOpen?.(reference.filePath, {
						startLine: reference.startLine,
						endLine: reference.endLine,
					})
				}
			/>
			{/* Render file reference cards */}
			{fileRefs.length > 0 && workspacePath && (
				<div className="flex flex-wrap gap-2 mt-2">
					{fileRefs.map((ref) => (
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
			className="inline-flex items-center gap-2 px-3 py-1.5 border border-border bg-muted/20 rounded hover:bg-muted/40 transition-colors text-sm"
		>
			<FileIcon className="w-4 h-4 text-muted-foreground" />
			<span className="font-medium">{fileName}</span>
			<span className="text-xs text-muted-foreground">
				{workspaceScopedPath}
			</span>
			<ExternalLink className="w-3 h-3 text-muted-foreground" />
		</button>
	);
});
