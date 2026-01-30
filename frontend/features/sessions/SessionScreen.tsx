"use client";

import {
	A2UICallCard,
	AgentMentionPopup,
	type AgentTarget,
	AgentTargetChip,
	type FileAttachment,
	FileAttachmentChip,
	FileMentionPopup,
	type IssueAttachment,
	IssueAttachmentChip,
	PermissionBanner,
	PermissionDialog,
	ReadAloudButton,
	SlashCommandPopup,
	type TodoItem,
	ToolCallCard,
	UserQuestionBanner,
	UserQuestionDialog,
} from "@/components/chat";
import { BrailleSpinner } from "@/components/common";
import { useUIControl } from "@/components/contexts/ui-control-context";
import {
	ContextWindowGauge,
	CopyButton,
	MarkdownRenderer,
	ProviderIcon,
} from "@/components/data-display";
import {
	ChatSearchBar,
	MainChatPiView,
	PiSettingsView,
} from "@/components/main-chat";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
	ContextMenu,
	ContextMenuContent,
	ContextMenuItem,
	ContextMenuSeparator,
	ContextMenuTrigger,
} from "@/components/ui/context-menu";
import { Input } from "@/components/ui/input";
import {
	Select,
	SelectContent,
	SelectItem,
	SelectTrigger,
	SelectValue,
} from "@/components/ui/select";
import {
	DictationOverlay,
	VoiceInputOverlay,
	VoiceMenuButton,
	type VoiceMode,
	VoicePanel,
} from "@/components/voice";
import {
	type PiModelInfo,
	type PiState,
	getWorkspacePiModels,
	getWorkspacePiState,
	setWorkspacePiModel,
} from "@/features/main-chat/api";
import {
	type Features,
	type MainChatSession,
	type Persona,
	type SessionAutoAttachMode,
	askAgent,
	controlPlaneDirectBaseUrl,
	convertChatMessagesToOpenCode,
	fileserverWorkspaceBaseUrl,
	getAuthHeaders,
	getChatMessages,
	getFeatures,
	getMainChatAssistant,
	getOrCreateSessionForWorkspace,
	getProjectLogoUrl,
	getWorkspaceConfig,
	listMainChatSessions,
	opencodeProxyBaseUrl,
	registerMainChatSession,
	touchSessionActivity,
	workspaceFileUrl,
} from "@/features/sessions/api";
import {
	type OpenCodeAssistantMessage,
	type OpenCodeMessageWithParts,
	type OpenCodePart,
	type OpenCodePartInput,
	type Permission,
	type PermissionResponse,
	type QuestionAnswer,
	type QuestionRequest,
	abortSession,
	createSession,
	fetchAgents,
	fetchCommands,
	fetchMessages,
	fetchProviders,
	fetchSessions,
	forkSession,
	invalidateMessageCache,
	rejectQuestion,
	replyToQuestion,
	respondToPermission,
	runShellCommandAsync,
	sendCommandAsync,
	sendMessageAsync,
	sendPartsAsync,
} from "@/features/sessions/api";
import {
	type FileTreeState,
	FileTreeView,
	initialFileTreeState,
} from "@/features/sessions/components/FileTreeView";
import type { MessageGroup, ThreadedMessage } from "@/features/sessions/types";
import { fetchMainChatThreadedMessages } from "@/features/sessions/utils/fetchMainChatThreadedMessages";
import { groupMessages } from "@/features/sessions/utils/groupMessages";
import { mergeSessionMessages } from "@/features/sessions/utils/mergeSessionMessages";
import { type A2UISurfaceState, useA2UI } from "@/hooks/use-a2ui";
import { useApp } from "@/hooks/use-app";
import { useDictation } from "@/hooks/use-dictation";
import { useIsMobile } from "@/hooks/use-mobile";
import { useModelContextLimit } from "@/hooks/use-models-dev";
import { useSessionEvents } from "@/hooks/use-session-events";
import {
	useVoiceCommandListener,
	useVoiceShortcuts,
} from "@/hooks/use-voice-commands";
import { useVoiceMode } from "@/hooks/use-voice-mode";
import type { A2UIUserAction } from "@/lib/a2ui/types";
import { extractFileReferences, getFileTypeInfo } from "@/lib/file-types";
import { getMessageText } from "@/lib/message-text";
import { type ModelOption, filterModelOptions } from "@/lib/model-filter";
import { normalizePermissionEvent } from "@/lib/session-events";
import { formatSessionDate, resolveReadableId } from "@/lib/session-utils";
import {
	type SlashCommand,
	builtInCommands,
	commandInfoToSlashCommands,
	fuzzyMatch,
	parseSlashInput,
} from "@/lib/slash-commands";
import { cn } from "@/lib/utils";
import type { WsEvent } from "@/lib/ws-client";
import {
	ArrowDown,
	AudioLines,
	Bot,
	Brain,
	Check,
	CheckSquare,
	ChevronDown,
	ChevronUp,
	CircleDot,
	Clock,
	Copy,
	FileCode,
	FileImage,
	FileText,
	FileVideo,
	GitBranch,
	ListTodo,
	Loader2,
	Maximize2,
	MessageSquare,
	Mic,
	Minimize2,
	PaintBucket,
	PanelLeftClose,
	PanelRightClose,
	Paperclip,
	RefreshCw,
	Search,
	Send,
	Settings,
	Sparkles,
	Square,
	StopCircle,
	Terminal,
	User,
	X,
	XCircle,
} from "lucide-react";
import {
	Profiler,
	Suspense,
	lazy,
	memo,
	startTransition,
	useCallback,
	useDeferredValue,
	useEffect,
	useLayoutEffect,
	useMemo,
	useRef,
	useState,
	useTransition,
} from "react";
import { toast } from "sonner";

function isPerfDebugEnabled(): boolean {
	if (!import.meta.env.DEV) return false;
	try {
		return localStorage.getItem("debug:perf") === "1";
	} catch {
		return false;
	}
}

const PreviewView = lazy(() =>
	import("@/features/sessions/components/PreviewView").then((mod) => ({
		default: mod.PreviewView,
	})),
);
const TerminalView = lazy(() =>
	import("@/features/sessions/components/TerminalView").then((mod) => ({
		default: mod.TerminalView,
	})),
);
const MemoriesView = lazy(() =>
	import("@/features/sessions/components/MemoriesView").then((mod) => ({
		default: mod.MemoriesView,
	})),
);
const AgentSettingsView = lazy(() =>
	import("@/features/sessions/components/AgentSettingsView").then((mod) => ({
		default: mod.AgentSettingsView,
	})),
);
const TrxView = lazy(() =>
	import("@/features/sessions/components/TrxView").then((mod) => ({
		default: mod.TrxView,
	})),
);
const CanvasView = lazy(() =>
	import("@/features/sessions/components/CanvasView").then((mod) => ({
		default: mod.CanvasView,
	})),
);

// ThreadedMessage and MessageGroup live in features/sessions/types.

type ActiveView =
	| "chat"
	| "files"
	| "terminal"
	| "tasks"
	| "memories"
	| "voice"
	| "settings"
	| "canvas";

type ExpandedView = "preview" | "canvas" | "memories" | "terminal" | null;
type TasksSubTab = "todos" | "planner";

type ChatMessagesPaneProps = {
	messages: OpenCodeMessageWithParts[];
	messagesLoading: boolean;
	selectedChatSessionId: string | undefined;
	sessionHadMessages: boolean;
	hasHiddenMessages: boolean;
	messageGroupsLength: number;
	visibleGroups: MessageGroup[];
	visibleGroupCount: number;
	a2uiByGroupIndex: Map<number, A2UISurfaceState[]>;
	locale: "de" | "en";
	noMessagesText: string;
	persona?: Persona | null;
	workspaceName?: string | null;
	readableId?: string | null;
	workspaceDirectory?: string;
	onFork?: (messageId: string) => void;
	onScroll: () => void;
	messagesContainerRef: { current: HTMLDivElement | null };
	messagesEndRef: { current: HTMLDivElement | null };
	showScrollToBottom: boolean;
	scrollToBottom: (behavior?: ScrollBehavior) => void;
	loadMoreMessages: () => void;
	onA2UIAction?: (action: A2UIUserAction) => void;
	isStreaming?: boolean;
};

const ChatMessagesPane = memo(function ChatMessagesPane({
	messages,
	messagesLoading,
	selectedChatSessionId,
	sessionHadMessages,
	hasHiddenMessages,
	messageGroupsLength,
	visibleGroups,
	visibleGroupCount,
	a2uiByGroupIndex,
	locale,
	noMessagesText,
	persona,
	workspaceName,
	readableId,
	workspaceDirectory,
	onFork,
	onScroll,
	messagesContainerRef,
	messagesEndRef,
	showScrollToBottom,
	scrollToBottom,
	loadMoreMessages,
	onA2UIAction,
	isStreaming,
}: ChatMessagesPaneProps) {
	return (
		<>
			<div
				ref={messagesContainerRef}
				onScroll={onScroll}
				className="h-full bg-muted/30 border border-border p-2 sm:p-4 overflow-y-auto scrollbar-hide"
				data-spotlight="chat-timeline"
			>
				{messages.length === 0 &&
					messagesLoading &&
					selectedChatSessionId &&
					sessionHadMessages && (
						<div className="animate-pulse">
							{/* User message skeleton */}
							<div className="sm:ml-8 bg-primary/10 border border-primary/20">
								<div className="flex items-center gap-2 px-2 sm:px-3 py-1.5 sm:py-2 border-b border-primary/20">
									<div className="w-3 h-3 sm:w-4 sm:h-4 bg-primary/30" />
									<div className="h-3 bg-primary/30 w-12" />
									<div className="flex-1" />
									<div className="h-2 bg-primary/20 w-10" />
								</div>
								<div className="px-2 sm:px-4 py-2 sm:py-3 space-y-2">
									<div className="h-3 bg-primary/20 w-3/4" />
									<div className="h-3 bg-primary/20 w-1/2" />
								</div>
							</div>
							{/* Assistant message skeleton */}
							<div className="mt-4 sm:mt-6 sm:mr-8 bg-muted/50 border border-border">
								<div className="flex items-center gap-2 px-2 sm:px-3 py-1.5 sm:py-2 border-b border-border">
									<div className="w-3 h-3 sm:w-4 sm:h-4 bg-muted" />
									<div className="h-3 bg-muted w-16" />
									<div className="flex-1" />
									<div className="h-2 bg-muted/70 w-10" />
								</div>
								<div className="px-2 sm:px-4 py-2 sm:py-3 space-y-2">
									<div className="h-3 bg-muted w-full" />
									<div className="h-3 bg-muted w-5/6" />
									<div className="h-3 bg-muted w-4/5" />
									<div className="h-3 bg-muted w-2/3" />
								</div>
							</div>
							{/* Another user message skeleton */}
							<div className="mt-4 sm:mt-6 sm:ml-8 bg-primary/10 border border-primary/20">
								<div className="flex items-center gap-2 px-2 sm:px-3 py-1.5 sm:py-2 border-b border-primary/20">
									<div className="w-3 h-3 sm:w-4 sm:h-4 bg-primary/30" />
									<div className="h-3 bg-primary/30 w-12" />
									<div className="flex-1" />
									<div className="h-2 bg-primary/20 w-10" />
								</div>
								<div className="px-2 sm:px-4 py-2 sm:py-3 space-y-2">
									<div className="h-3 bg-primary/20 w-2/3" />
								</div>
							</div>
						</div>
					)}

				{messages.length === 0 && !messagesLoading && !sessionHadMessages && (
					<div className="text-sm text-muted-foreground">{noMessagesText}</div>
				)}

				{hasHiddenMessages && (
					<button
						type="button"
						onClick={loadMoreMessages}
						className="w-full py-2 text-xs text-muted-foreground hover:text-foreground hover:bg-muted/50 border border-dashed border-border transition-colors"
					>
						{locale === "de"
							? `${messageGroupsLength - visibleGroupCount} altere Nachrichten laden...`
							: `Load ${messageGroupsLength - visibleGroupCount} older messages...`}
					</button>
				)}

				{/* Message groups with A2UI surfaces embedded */}
				{visibleGroups.map((group, groupIndex) => {
					// Check if this is the last assistant group (for showing working indicator)
					const isLastAssistantGroup =
						group.role === "assistant" &&
						!visibleGroups
							.slice(groupIndex + 1)
							.some((g) => g.role === "assistant");
					return (
						<div
							key={
								group.messages[0]?.info.id ||
								`${group.role}-${group.startIndex}`
							}
							className={groupIndex > 0 ? "mt-4 sm:mt-6" : ""}
						>
							{/* Session divider for Main Chat threaded view */}
							{group.isNewSession && group.sessionTitle && (
								<SessionDivider title={group.sessionTitle} />
							)}
							<MessageGroupCard
								group={group}
								persona={persona}
								workspaceName={workspaceName}
								readableId={readableId}
								workspaceDirectory={workspaceDirectory}
								onFork={onFork}
								locale={locale}
								a2uiSurfaces={a2uiByGroupIndex.get(groupIndex)}
								onA2UIAction={onA2UIAction}
								messageId={group.messages[0]?.info.id}
								showWorkingIndicator={isStreaming && isLastAssistantGroup}
							/>
						</div>
					);
				})}

				<div ref={messagesEndRef} data-messages-end />
			</div>

			{/* Jump to bottom button */}
			{showScrollToBottom && (
				<button
					type="button"
					onClick={() => scrollToBottom()}
					className="absolute bottom-2 left-2 right-2 sm:left-1/2 sm:-translate-x-1/2 sm:right-auto sm:w-auto z-50 flex items-center justify-center gap-2 px-3 py-2 bg-primary hover:bg-primary/90 text-primary-foreground text-sm font-medium shadow-lg"
				>
					<ArrowDown className="w-4 h-4" />
					<span className="sm:inline">Jump to bottom</span>
				</button>
			)}
		</>
	);
});

// groupMessages moved to features/sessions/utils/groupMessages.

// Session divider for Main Chat threaded view
function SessionDivider({ title }: { title: string }) {
	return (
		<div className="flex items-center gap-3 py-3 px-2">
			<div className="flex-1 h-px bg-border" />
			<div className="flex items-center gap-2 text-xs text-muted-foreground">
				<MessageSquare className="w-3 h-3" />
				<span className="font-medium">{title}</span>
			</div>
			<div className="flex-1 h-px bg-border" />
		</div>
	);
}

function TabButton({
	activeView,
	onSelect,
	view,
	icon: Icon,
	label,
	badge,
	hideLabel,
}: {
	activeView: ActiveView;
	onSelect: (view: ActiveView) => void;
	view: ActiveView;
	icon: React.ComponentType<{ className?: string }>;
	label: string;
	badge?: number;
	hideLabel?: boolean;
}) {
	return (
		<button
			type="button"
			onClick={() => onSelect(view)}
			className={cn(
				"flex-1 flex items-center justify-center px-1.5 py-1 relative transition-colors",
				activeView === view
					? "bg-primary/15 text-foreground border border-primary"
					: "text-muted-foreground border border-transparent hover:border-border hover:bg-muted/50",
			)}
			title={label}
		>
			<Icon className="w-4 h-4" />
			{!hideLabel && (
				<span className="hidden sm:inline ml-1 text-xs">{label}</span>
			)}
			{badge !== undefined && badge > 0 && (
				<span className="absolute -top-1 -right-1 w-3.5 h-3.5 bg-pink-500 text-white text-[9px] rounded-full flex items-center justify-center">
					{badge}
				</span>
			)}
		</button>
	);
}

// Collapsed sidebar tab button - vertical stacked icon
function CollapsedTabButton({
	activeView,
	onSelect,
	view,
	icon: Icon,
	label,
	badge,
}: {
	activeView: ActiveView;
	onSelect: (view: ActiveView) => void;
	view: ActiveView;
	icon: React.ComponentType<{ className?: string }>;
	label: string;
	badge?: number;
}) {
	return (
		<button
			type="button"
			onClick={() => onSelect(view)}
			className={cn(
				"w-8 h-8 flex items-center justify-center relative transition-colors rounded",
				activeView === view
					? "bg-primary/15 text-foreground border border-primary"
					: "text-muted-foreground border border-transparent hover:border-border hover:bg-muted/50",
			)}
			title={label}
		>
			<Icon className="w-4 h-4" />
			{badge !== undefined && badge > 0 && (
				<span className="absolute -top-1 -right-1 w-3.5 h-3.5 bg-pink-500 text-white text-[9px] rounded-full flex items-center justify-center">
					{badge}
				</span>
			)}
		</button>
	);
}

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

function parseModelRef(
	value: string,
): { providerID: string; modelID: string } | null {
	const trimmed = value.trim();
	const separatorIndex = trimmed.indexOf("/");
	if (separatorIndex <= 0 || separatorIndex === trimmed.length - 1) {
		return null;
	}
	return {
		providerID: trimmed.slice(0, separatorIndex),
		modelID: trimmed.slice(separatorIndex + 1),
	};
}

function isPendingSessionId(id: string | null | undefined): boolean {
	return !!id && id.startsWith("pending-");
}

export const SessionScreen = memo(function SessionScreen() {
	const {
		locale,
		workspaceSessions,
		selectedWorkspaceSessionId,
		setSelectedWorkspaceSessionId,
		selectedWorkspaceSession,
		opencodeBaseUrl,
		selectedChatSessionId,
		setSelectedChatSessionId,
		selectedChatSession,
		selectedChatFromHistory,
		refreshOpencodeSessions,
		refreshWorkspaceSessions,
		refreshChatHistory,
		ensureOpencodeRunning,
		chatHistory,
		projects,
		startProjectSession,
		setSessionBusy,
		mainChatActive,
		mainChatAssistantName,
		mainChatCurrentSessionId,
		setMainChatCurrentSessionId,
		mainChatWorkspacePath,
		setMainChatWorkspacePath,
		mainChatNewSessionTrigger,
		mainChatSessionActivityTrigger,
		notifyMainChatSessionActivity,
		scrollToMessageId,
		setScrollToMessageId,
	} = useApp();
	const { registerSessionControls } = useUIControl();
	const [messages, setMessages] = useState<OpenCodeMessageWithParts[]>([]);
	const [chatInputMountKey, setChatInputMountKey] = useState(0);
	const lastActiveChatSessionRef = useRef<string | null>(null);
	const lastActiveOpencodeBaseUrlRef = useRef<string>("");
	// Ref to track messages for A2UI anchoring
	const messagesRef = useRef(messages);
	useEffect(() => {
		messagesRef.current = messages;
	}, [messages]);
	// Use ref for input value to avoid re-renders on every keystroke
	// Only sync to state when needed for deferred computations
	const messageInputRef = useRef("");
	const [messageInputState, setMessageInputState] = useState("");
	// Debounce state sync to avoid blocking
	const inputSyncTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(
		null,
	);
	const syncInputToState = useCallback((value: string) => {
		messageInputRef.current = value;
		if (inputSyncTimeoutRef.current) {
			clearTimeout(inputSyncTimeoutRef.current);
		}
		// Sync to state after 100ms for deferred computations
		inputSyncTimeoutRef.current = setTimeout(() => {
			startTransition(() => {
				setMessageInputState(value);
			});
		}, 100);
	}, []);
	// For backward compatibility - direct access uses ref, deferred uses state
	const messageInput = messageInputState;
	const setMessageInput = useCallback(
		(value: string) => {
			messageInputRef.current = value;
			// Update textarea directly
			if (chatInputRef.current) {
				chatInputRef.current.value = value;
			}
			// Sync to state for derived values
			syncInputToState(value);
		},
		[syncInputToState],
	);

	const perfEnabled = isPerfDebugEnabled();
	const perfReasonRef = useRef<string>("");
	const onProfilerRender = useCallback(
		(
			id: string,
			phase: "mount" | "update" | "nested-update",
			actualDuration: number,
			baseDuration: number,
			startTime: number,
			commitTime: number,
		) => {
			if (!perfEnabled) return;
			if (actualDuration < 16) return;
			console.debug("[perf] render", {
				id,
				phase,
				actualDuration: Math.round(actualDuration),
				baseDuration: Math.round(baseDuration),
				reason: perfReasonRef.current,
				startTime: Math.round(startTime),
				commitTime: Math.round(commitTime),
			});
			perfReasonRef.current = "";
		},
		[perfEnabled],
	);

	const chatInputResizeRef = useRef<{
		raf: number | null;
		value: string;
	}>({ raf: null, value: "" });

	// Helper to set message input and resize textarea.
	// Coalesce resize work to a single RAF to avoid reflow storms while typing.
	const setMessageInputWithResize = useCallback(
		(value: string) => {
			messageInputRef.current = value;
			if (chatInputRef.current) {
				chatInputRef.current.value = value;
			}
			syncInputToState(value);
			chatInputResizeRef.current.value = value;

			if (chatInputResizeRef.current.raf !== null) return;

			chatInputResizeRef.current.raf = requestAnimationFrame(() => {
				chatInputResizeRef.current.raf = null;
				const textarea = chatInputRef.current;
				if (!textarea) return;

				const currentValue = chatInputResizeRef.current.value;
				// Reset to base height first, then expand if needed.
				textarea.style.height = "36px";
				if (currentValue) {
					textarea.style.height = `${Math.min(textarea.scrollHeight, 200)}px`;
				}
			});
		},
		[syncInputToState],
	);

	const [mainChatBaseUrl, setMainChatBaseUrl] = useState("");
	const opencodeDirectory = useMemo(() => {
		if (mainChatActive) return mainChatWorkspacePath ?? undefined;
		return (
			selectedChatFromHistory?.workspace_path ??
			selectedWorkspaceSession?.workspace_path
		);
	}, [
		mainChatActive,
		mainChatWorkspacePath,
		selectedChatFromHistory,
		selectedWorkspaceSession,
	]);
	const opencodeRequestOptions = useMemo(
		() => ({ directory: opencodeDirectory }),
		[opencodeDirectory],
	);
	const effectiveOpencodeBaseUrl = useMemo(() => {
		if (mainChatActive && mainChatBaseUrl) return mainChatBaseUrl;
		return opencodeBaseUrl;
	}, [mainChatActive, mainChatBaseUrl, opencodeBaseUrl]);
	useEffect(() => {
		if (effectiveOpencodeBaseUrl) {
			lastActiveOpencodeBaseUrlRef.current = effectiveOpencodeBaseUrl;
		}
	}, [effectiveOpencodeBaseUrl]);

	const [opencodeModelOptions, setOpencodeModelOptions] = useState<
		ModelOption[]
	>([]);
	const [selectedModelRef, setSelectedModelRef] = useState<string | null>(null);
	const [isModelLoading, setIsModelLoading] = useState(false);
	const [modelQuery, setModelQuery] = useState("");
	const [piModelOptions, setPiModelOptions] = useState<PiModelInfo[]>([]);
	const [piSelectedModelRef, setPiSelectedModelRef] = useState<string | null>(
		null,
	);
	const [piModelQuery, setPiModelQuery] = useState("");
	const [piIsModelLoading, setPiIsModelLoading] = useState(false);
	const [piIsSwitchingModel, setPiIsSwitchingModel] = useState(false);
	const [piState, setPiState] = useState<PiState | null>(null);
	const modelStorageKey = useMemo(() => {
		if (!selectedChatSessionId || mainChatActive) return null;
		return `octo:chatModel:${selectedChatSessionId}`;
	}, [selectedChatSessionId, mainChatActive]);

	// Track previous storage key to avoid saving stale model to new session
	const prevModelStorageKeyRef = useRef<string | null>(null);

	useEffect(() => {
		if (!modelStorageKey) {
			setSelectedModelRef(null);
			prevModelStorageKeyRef.current = null;
			return;
		}
		const stored = localStorage.getItem(modelStorageKey);
		setSelectedModelRef(stored || null);
		prevModelStorageKeyRef.current = modelStorageKey;
	}, [modelStorageKey]);

	useEffect(() => {
		// Only save if the storage key hasn't changed (avoid saving old model to new session)
		if (!modelStorageKey || prevModelStorageKeyRef.current !== modelStorageKey)
			return;
		if (selectedModelRef) {
			localStorage.setItem(modelStorageKey, selectedModelRef);
		} else {
			localStorage.removeItem(modelStorageKey);
		}
	}, [modelStorageKey, selectedModelRef]);

	useEffect(() => {
		if (!effectiveOpencodeBaseUrl || mainChatActive) {
			if (!effectiveOpencodeBaseUrl) {
				console.debug("[Models] No opencode URL, clearing model options");
			}
			setOpencodeModelOptions([]);
			setIsModelLoading(false);
			return;
		}
		let active = true;
		setIsModelLoading(true);
		fetchProviders(effectiveOpencodeBaseUrl, { directory: opencodeDirectory })
			.then((data) => {
				if (!active) return;
				const providers = data.providers ?? data.all ?? [];
				const options: ModelOption[] = [];
				for (const provider of providers) {
					const models = provider.models ?? {};
					for (const [modelKey, model] of Object.entries(models)) {
						const modelId = model.id || modelKey;
						if (!modelId) continue;
						const value = `${provider.id}/${modelId}`;
						const label = model.name
							? `${provider.id}/${modelId} · ${model.name}`
							: value;
						options.push({ value, label });
					}
				}
				options.sort((a, b) => a.label.localeCompare(b.label));
				setOpencodeModelOptions(options);
			})
			.catch((err) => {
				console.error("Failed to fetch models:", err);
				if (active) setOpencodeModelOptions([]);
			})
			.finally(() => {
				if (active) setIsModelLoading(false);
			});

		return () => {
			active = false;
		};
	}, [effectiveOpencodeBaseUrl, opencodeDirectory, mainChatActive]);

	// Main chat token usage (for mobile gauge)
	const [mainChatTokenUsage, setMainChatTokenUsage] = useState<{
		inputTokens: number;
		outputTokens: number;
		maxTokens: number;
	}>({ inputTokens: 0, outputTokens: 0, maxTokens: 200000 });
	// Workspace Pi token usage (for sessions rendered via Pi)
	const [workspacePiTokenUsage, setWorkspacePiTokenUsage] = useState<{
		inputTokens: number;
		outputTokens: number;
		maxTokens: number;
	}>({ inputTokens: 0, outputTokens: 0, maxTokens: 200000 });

	// Per-chat state (working indicator is per-session, not global)
	const [chatStates, setChatStates] = useState<Map<string, "idle" | "sending">>(
		new Map(),
	);
	// In Main Chat mode, use mainChatCurrentSessionId; otherwise use selectedChatSessionId
	const activeSessionId = mainChatActive
		? mainChatCurrentSessionId
		: selectedChatSessionId;
	const chatState = activeSessionId
		? chatStates.get(activeSessionId) || "idle"
		: "idle";
	const isWorkspacePiSession =
		!mainChatActive &&
		!!selectedChatSessionId &&
		!selectedChatSessionId.startsWith("ses_");
	const workspacePiPath = useMemo(() => {
		if (!isWorkspacePiSession) return null;
		return (
			selectedChatFromHistory?.workspace_path ??
			selectedWorkspaceSession?.workspace_path ??
			opencodeDirectory ??
			null
		);
	}, [
		isWorkspacePiSession,
		opencodeDirectory,
		selectedChatFromHistory,
		selectedWorkspaceSession,
	]);
	const workspacePiStorageKeyPrefix = useMemo(() => {
		if (!workspacePiPath) return "octo:workspacePi:global";
		return `octo:workspacePi:${workspacePiPath.replace(/[^a-zA-Z0-9._-]+/g, "_")}`;
	}, [workspacePiPath]);
	const handleWorkspacePiSessionChange = useCallback(
		(id: string | null) => {
			if (!id) return;
			setSelectedChatSessionId(id);
			refreshChatHistory();
		},
		[refreshChatHistory, setSelectedChatSessionId],
	);

	useEffect(() => {
		if (
			!isWorkspacePiSession ||
			!selectedChatSessionId ||
			!workspacePiPath ||
			isPendingSessionId(selectedChatSessionId)
		) {
			setPiModelOptions([]);
			setPiIsModelLoading(false);
			return;
		}
		let active = true;
		setPiIsModelLoading(true);
		getWorkspacePiModels(workspacePiPath, selectedChatSessionId)
			.then((models) => {
				if (!active) return;
				setPiModelOptions(models);
			})
			.catch((err) => {
				console.error("Failed to fetch Pi models:", err);
				if (active) setPiModelOptions([]);
			})
			.finally(() => {
				if (active) setPiIsModelLoading(false);
			});
		return () => {
			active = false;
		};
	}, [isWorkspacePiSession, selectedChatSessionId, workspacePiPath]);

	useEffect(() => {
		if (piSelectedModelRef || piModelOptions.length === 0) return;
		const first = piModelOptions[0];
		setPiSelectedModelRef(`${first.provider}/${first.id}`);
	}, [piModelOptions, piSelectedModelRef]);

	useEffect(() => {
		let active = true;
		let intervalId: ReturnType<typeof setInterval> | null = null;

		const fetchState = async () => {
			if (!active) return;
			try {
				if (
					!workspacePiPath ||
					!selectedChatSessionId ||
					isPendingSessionId(selectedChatSessionId)
				) {
					setPiState(null);
					return;
				}
				const nextState = await getWorkspacePiState(
					workspacePiPath,
					selectedChatSessionId,
				);
				if (active) setPiState(nextState);
			} catch {
				if (active) setPiState(null);
			}
		};

		if (
			isWorkspacePiSession &&
			selectedChatSessionId &&
			workspacePiPath &&
			!isPendingSessionId(selectedChatSessionId)
		) {
			void fetchState();
			intervalId = setInterval(fetchState, 2000);
		} else {
			setPiState(null);
		}

		return () => {
			active = false;
			if (intervalId) clearInterval(intervalId);
		};
	}, [isWorkspacePiSession, selectedChatSessionId, workspacePiPath]);

	useEffect(() => {
		if (!piState?.model) return;
		const modelRef = `${piState.model.provider}/${piState.model.id}`;
		setPiSelectedModelRef(modelRef);
	}, [piState?.model]);
	const selectedModelOverride = useMemo(() => {
		if (!selectedModelRef) return undefined;
		return parseModelRef(selectedModelRef) ?? undefined;
	}, [selectedModelRef]);
	const piIsIdle = !(piState?.is_streaming || piState?.is_compacting);
	const filteredPiModels = useMemo(() => {
		const query = piModelQuery.trim();
		if (!query) return piModelOptions;
		return piModelOptions.filter((model) => {
			const fullRef = `${model.provider}/${model.id}`;
			return (
				fuzzyMatch(query, fullRef) ||
				fuzzyMatch(query, model.provider) ||
				fuzzyMatch(query, model.id) ||
				(model.name ? fuzzyMatch(query, model.name) : false)
			);
		});
	}, [piModelOptions, piModelQuery]);
	const handlePiModelChange = useCallback(
		async (value: string) => {
			if (
				!piIsIdle ||
				!workspacePiPath ||
				!selectedChatSessionId ||
				isPendingSessionId(selectedChatSessionId)
			) {
				return;
			}
			const separatorIndex = value.indexOf("/");
			if (separatorIndex <= 0 || separatorIndex === value.length - 1) return;
			const provider = value.slice(0, separatorIndex);
			const modelId = value.slice(separatorIndex + 1);
			setPiSelectedModelRef(value);
			setPiIsSwitchingModel(true);
			try {
				await setWorkspacePiModel(
					workspacePiPath,
					selectedChatSessionId,
					provider,
					modelId,
				);
				const refreshed = await getWorkspacePiState(
					workspacePiPath,
					selectedChatSessionId,
				);
				setPiState(refreshed);
			} catch (err) {
				console.error("Failed to switch Pi model:", err);
			} finally {
				setPiIsSwitchingModel(false);
			}
		},
		[piIsIdle, selectedChatSessionId, workspacePiPath],
	);
	const setChatState = useCallback(
		(state: "idle" | "sending") => {
			const sessionId = mainChatActive
				? mainChatCurrentSessionId
				: selectedChatSessionId;
			if (!sessionId) return;
			setChatStates((prev) => {
				const next = new Map(prev);
				next.set(sessionId, state);
				return next;
			});
			// Also update global busy state for sidebar indicator
			setSessionBusy(sessionId, state === "sending");
		},
		[
			selectedChatSessionId,
			mainChatActive,
			mainChatCurrentSessionId,
			setSessionBusy,
		],
	);

	useEffect(() => {
		if (!mainChatActive && mainChatBaseUrl) {
			setMainChatBaseUrl("");
		}
	}, [mainChatActive, mainChatBaseUrl]);

	useEffect(() => {
		if (!mainChatActive || !mainChatAssistantName || mainChatWorkspacePath)
			return;
		let cancelled = false;
		getMainChatAssistant(mainChatAssistantName)
			.then((info) => {
				if (!cancelled) {
					setMainChatWorkspacePath(info.path);
				}
			})
			.catch((err) => {
				console.error("Failed to load Main Chat workspace path:", err);
			});
		return () => {
			cancelled = true;
		};
	}, [
		mainChatActive,
		mainChatAssistantName,
		mainChatWorkspacePath,
		setMainChatWorkspacePath,
	]);

	// Per-chat draft text cache (persists across session switches AND component remounts via localStorage)
	const previousSessionIdRef = useRef<string | null>(null);
	// Debounce timer for saving drafts to localStorage
	const draftSaveTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(
		null,
	);
	const draftWriteTokenRef = useRef(0);

	// Helper to get/set drafts from localStorage
	const getDraft = useCallback((sessionId: string): string => {
		if (typeof window === "undefined") return "";
		try {
			const drafts = JSON.parse(
				localStorage.getItem("octo:chatDrafts") || "{}",
			);
			return drafts[sessionId] || "";
		} catch {
			return "";
		}
	}, []);

	const setDraft = useCallback((sessionId: string, text: string) => {
		if (typeof window === "undefined") return;
		try {
			const drafts = JSON.parse(
				localStorage.getItem("octo:chatDrafts") || "{}",
			);
			if (text.trim()) {
				drafts[sessionId] = text;
			} else {
				delete drafts[sessionId];
			}
			localStorage.setItem("octo:chatDrafts", JSON.stringify(drafts));
		} catch {
			// Ignore localStorage errors
		}
	}, []);

	// Save and restore drafts when switching sessions
	useEffect(() => {
		const prevId = previousSessionIdRef.current;
		const currId = selectedChatSessionId;

		// When switching sessions, save the outgoing draft and restore the incoming one
		if (currId !== prevId) {
			// Save current input as draft for the previous session (if any)
			if (prevId && messageInputRef.current) {
				// Cancel any pending debounced save
				if (draftSaveTimeoutRef.current) {
					clearTimeout(draftSaveTimeoutRef.current);
					draftSaveTimeoutRef.current = null;
				}
				// Save immediately
				setDraft(prevId, messageInputRef.current);
			}

			// Restore draft for current session (or clear if none)
			if (currId) {
				const savedDraft = getDraft(currId);
				messageInputRef.current = savedDraft;
				if (chatInputRef.current) {
					chatInputRef.current.value = savedDraft;
				}
				syncInputToState(savedDraft);
				// Auto-resize after draft restoration
				requestAnimationFrame(() => {
					if (chatInputRef.current) {
						const textarea = chatInputRef.current;
						if (!savedDraft) {
							textarea.style.height = "36px";
						} else {
							textarea.style.height = "36px";
							textarea.style.height = `${Math.min(textarea.scrollHeight, 200)}px`;
						}
					}
				});
			}
		}

		previousSessionIdRef.current = currId;
	}, [selectedChatSessionId, getDraft, setDraft, syncInputToState]);

	// Save draft on unmount to prevent loss when navigating away
	useEffect(() => {
		return () => {
			// Save any pending draft when component unmounts
			if (draftSaveTimeoutRef.current) {
				clearTimeout(draftSaveTimeoutRef.current);
			}
			const sessionId = previousSessionIdRef.current;
			const currentInput = messageInputRef.current;
			if (sessionId && currentInput) {
				// Use sync localStorage write since we're unmounting
				try {
					const drafts = JSON.parse(
						localStorage.getItem("octo:chatDrafts") || "{}",
					);
					if (currentInput.trim()) {
						drafts[sessionId] = currentInput;
					} else {
						delete drafts[sessionId];
					}
					localStorage.setItem("octo:chatDrafts", JSON.stringify(drafts));
				} catch {
					// Ignore localStorage errors
				}
			}
		};
	}, []);

	const [isLoading, setIsLoading] = useState(true);
	const [messagesLoading, setMessagesLoading] = useState(false);
	const [showTimeoutError, setShowTimeoutError] = useState(false);
	const [activeView, setActiveView] = useState<ActiveView>("chat");
	const [tasksSubTab, setTasksSubTab] = useState<TasksSubTab>("todos");
	const [mainChatTodos, setMainChatTodos] = useState<TodoItem[]>([]);
	const [workspacePiTodos, setWorkspacePiTodos] = useState<TodoItem[]>([]);
	const [expandedView, setExpandedView] = useState<ExpandedView>(null);
	const [rightSidebarCollapsed, setRightSidebarCollapsed] = useState(false);
	const [isSearchOpen, setIsSearchOpen] = useState(false);
	const [status, setStatus] = useState<string>("");
	const [showScrollToBottom, setShowScrollToBottom] = useState(false);
	const [previewFilePath, setPreviewFilePath] = useState<string | null>(null);
	const [fileTreeState, setFileTreeState] =
		useState<FileTreeState>(initialFileTreeState);
	const messagesContainerRef = useRef<HTMLDivElement>(null);
	const messagesEndRef = useRef<HTMLDivElement>(null);

	useEffect(() => {
		registerSessionControls({
			setActiveView: (view) => setActiveView(view as ActiveView),
			setExpandedView: (view) => setExpandedView(view as ExpandedView),
			setRightSidebarCollapsed,
		});
		return () => registerSessionControls(null);
	}, [registerSessionControls]);

	// Track if auto-scroll is enabled (user hasn't scrolled away)
	const autoScrollEnabledRef = useRef(true);
	const lastSessionIdRef = useRef<string | null>(null);
	// Track last scroll position to detect scroll direction
	const lastScrollTopRef = useRef(0);
	// Track if this is initial message load (for instant scroll) vs streaming (for smooth scroll)
	const initialLoadRef = useRef(true);
	// Cache scroll positions per session (sessionId -> scrollTop, null means bottom)
	const scrollPositionCacheRef = useRef<Map<string, number | null>>(new Map());
	const pendingVoiceScrollRef = useRef(false);
	// Track sessions that have had messages loaded (to show skeleton only for sessions with history)
	const sessionsWithMessagesRef = useRef<Set<string>>(new Set());
	const autoAttachAttemptRef = useRef<{
		sessionId: string;
		workspacePath: string;
		mode: SessionAutoAttachMode;
	} | null>(null);
	const autoAttachScanAttemptRef = useRef<{
		sessionId: string;
		workspacePath: string;
		runningSessionIds: string;
	} | null>(null);
	const sessionUnavailableRef = useRef<{
		sessionId: string;
		attemptedAt: number;
	} | null>(null);
	// Track the session ID that loadMessages is currently loading for (to prevent stale updates)
	const loadingSessionIdRef = useRef<string | null>(null);
	// Track request counter to prevent race conditions when multiple fetches overlap
	const loadRequestCounterRef = useRef(0);
	const fileInputRef = useRef<HTMLInputElement>(null);
	const chatInputRef = useRef<HTMLTextAreaElement>(null);
	const chatContainerRef = useRef<HTMLDivElement>(null);
	const prevVoiceActiveRef = useRef(false);
	// Flag to ignore onChange events immediately after sending (prevents stale event restoration)
	const ignoringInputRef = useRef(false);

	// Stable ref callback to avoid resetting refs every render.
	// Always sync DOM value from `messageInputRef` (including empty string).
	const setChatInputEl = useCallback((el: HTMLTextAreaElement | null) => {
		chatInputRef.current = el;
		if (!el) return;

		if (ignoringInputRef.current) {
			el.value = "";
			return;
		}

		el.value = messageInputRef.current;
	}, []);

	// File upload state
	const [pendingUploads, setPendingUploads] = useState<
		{ name: string; path: string }[]
	>([]);

	// Slash command popup state
	const [showSlashPopup, setShowSlashPopup] = useState(false);
	const [slashCommands, setSlashCommands] =
		useState<SlashCommand[]>(builtInCommands);
	// Use deferred value for slash parsing to avoid blocking input
	const deferredMessageInput = useDeferredValue(messageInput);
	const slashQuery = useMemo(
		() => parseSlashInput(deferredMessageInput),
		[deferredMessageInput],
	);

	// File mention popup state
	const [showFileMentionPopup, setShowFileMentionPopup] = useState(false);
	const [fileMentionQuery, setFileMentionQuery] = useState("");
	const [fileAttachments, setFileAttachments] = useState<FileAttachment[]>([]);
	const [issueAttachments, setIssueAttachments] = useState<IssueAttachment[]>(
		[],
	);

	// Agent mention popup state (@@mentions)
	const [showAgentMentionPopup, setShowAgentMentionPopup] = useState(false);
	const [agentMentionQuery, setAgentMentionQuery] = useState("");
	const [agentTarget, setAgentTarget] = useState<AgentTarget | null>(null);
	// When true, agent ask will only show response in toast, not inject into current chat
	const agentAskNoReplyRef = useRef(false);

	// Default agent for shell commands - use "build" as the default primary agent
	const [defaultAgent, setDefaultAgent] = useState<string>("build");
	const [isUploading, setIsUploading] = useState(false);

	// Permission state
	const [pendingPermissions, setPendingPermissions] = useState<Permission[]>(
		[],
	);
	const [activePermission, setActivePermission] = useState<Permission | null>(
		null,
	);

	// Question state (for user question/multiple choice selection)
	const [pendingQuestions, setPendingQuestions] = useState<QuestionRequest[]>(
		[],
	);
	const [activeQuestion, setActiveQuestion] = useState<QuestionRequest | null>(
		null,
	);
	const [lastCompactionAt, setLastCompactionAt] = useState<number | null>(null);

	// A2UI surfaces using the modular hook
	const {
		surfaces: a2uiSurfaces,
		handleAction: handleA2UIAction,
		handleDismiss: handleA2UIDismiss,
		clearSurfaces: clearA2UISurfaces,
	} = useA2UI(messagesRef, {
		onSurfaceReceived: () => {
			// Auto-scroll to show new A2UI surface
			setTimeout(() => {
				const endEl = document.querySelector("[data-messages-end]");
				if (endEl) {
					endEl.scrollIntoView({ behavior: "smooth", block: "end" });
				} else {
					scrollToBottom("smooth");
				}
			}, 200);
		},
	});

	// Clear permission, question, and A2UI state when session changes
	// Note: Permissions/Questions/A2UI are received via WS events, not fetched via REST
	const prevSessionRef = useRef(selectedChatSessionId);
	useEffect(() => {
		if (prevSessionRef.current !== selectedChatSessionId) {
			prevSessionRef.current = selectedChatSessionId;
			setPendingPermissions([]);
			setActivePermission(null);
			setPendingQuestions([]);
			setActiveQuestion(null);
			clearA2UISurfaces();
			setLastCompactionAt(null);
		}
	});

	useEffect(() => {
		// Reset compaction marker when switching between Main Chat and session view.
		if (mainChatActive) {
			setLastCompactionAt(null);
		} else {
			setLastCompactionAt(null);
		}
	}, [mainChatActive]);

	// Track if we're on mobile layout (below lg breakpoint = 1024px)
	const isMobileLayout = useIsMobile();

	// Feature flags from backend
	const [features, setFeatures] = useState<Features>({ mmry_enabled: false });

	// Connection diagnostics for debugging (especially iOS)
	const [connectionDiagnostics, setConnectionDiagnostics] = useState<{
		controlPlaneUrl: string;
		lastError: string | null;
		lastAttempt: number | null;
		featuresLoaded: boolean;
	}>({
		controlPlaneUrl: controlPlaneDirectBaseUrl(),
		lastError: null,
		lastAttempt: null,
		featuresLoaded: false,
	});

	// Fetch features on mount
	useEffect(() => {
		const url = controlPlaneDirectBaseUrl();
		setConnectionDiagnostics((prev) => ({
			...prev,
			controlPlaneUrl: url,
			lastAttempt: Date.now(),
		}));

		getFeatures()
			.then((f) => {
				setFeatures(f);
				setConnectionDiagnostics((prev) => ({
					...prev,
					featuresLoaded: true,
					lastError: null,
				}));
			})
			.catch((err) => {
				// Capture error for diagnostics
				setConnectionDiagnostics((prev) => ({
					...prev,
					lastError: err instanceof Error ? err.message : String(err),
					featuresLoaded: false,
				}));
			});
	}, []);

	// Fetch slash commands from opencode when URL is available
	useEffect(() => {
		if (!opencodeBaseUrl) {
			setSlashCommands(builtInCommands);
			return;
		}

		fetchCommands(opencodeBaseUrl, opencodeRequestOptions)
			.then((commands) => {
				setSlashCommands(commandInfoToSlashCommands(commands));
			})
			.catch(() => {
				// Fall back to built-in commands
				setSlashCommands(builtInCommands);
			});
	}, [opencodeBaseUrl, opencodeRequestOptions]);

	// Ref for voice transcript send - will be set after handleSend is defined
	const voiceSendRef = useRef<() => void>(() => {});

	// Voice mode - handles STT/TTS when voice feature is enabled
	const handleVoiceTranscript = useCallback(
		(text: string) => {
			// Set the transcript as message input
			setMessageInputWithResize(text);
			// Call send via ref to avoid stale closure issues
			// Small delay to ensure ref value is set in textarea
			setTimeout(() => {
				voiceSendRef.current();
			}, 50);
		},
		[setMessageInputWithResize],
	);

	const voiceMode = useVoiceMode({
		config: features.voice ?? null,
		onTranscript: handleVoiceTranscript,
	});

	// Dictation mode - speech to text for the input field
	// messageInputRef is already defined above for uncontrolled input

	const handleDictationTranscript = useCallback(
		(text: string) => {
			// Always append to the current value using the ref to avoid stale closures.
			// Keep the textarea stable during dictation; the dictation overlay is the input UI.
			const currentValue = messageInputRef.current;
			const newValue = currentValue ? `${currentValue} ${text}` : text;
			messageInputRef.current = newValue;
			if (chatInputRef.current) {
				chatInputRef.current.value = newValue;
			}
			syncInputToState(newValue);
		},
		[syncInputToState],
	);

	const dictation = useDictation({
		config: features.voice ?? null,
		onTranscript: handleDictationTranscript,
		vadTimeoutMs: 3000,
		autoSendOnFinal: true,
		autoSendDelayMs: 50,
		onAutoSend: () => {
			const sendBtn = document.querySelector(
				"[data-voice-send]",
			) as HTMLButtonElement | null;
			sendBtn?.click();
		},
	});

	// Listen for voice commands from command palette and keyboard shortcuts
	useVoiceCommandListener(
		useCallback(
			(command) => {
				if (!features.voice) return;

				switch (command) {
					case "conversation":
						if (dictation.isActive) dictation.stop();
						if (!voiceMode.isActive) voiceMode.start().catch(console.error);
						break;
					case "dictation":
						if (voiceMode.isActive) voiceMode.stop();
						if (!dictation.isActive) dictation.start().catch(console.error);
						break;
					case "stop":
						if (voiceMode.isActive) voiceMode.stop();
						if (dictation.isActive) dictation.stop();
						break;
				}
			},
			[features.voice, voiceMode, dictation],
		),
	);

	// Register global keyboard shortcuts for voice (Alt+V, Alt+D)
	useVoiceShortcuts(!!features.voice);

	// Streaming TTS: Track the active stream for current message
	const ttsStreamStateRef = useRef<{
		messageId: string | null;
		streamId: string | null;
		sentLength: number; // How many characters we've already sent
	}>({ messageId: null, streamId: null, sentLength: 0 });
	const voiceActivationRef = useRef(false);

	// Seed TTS stream state when voice mode is activated to avoid reading history
	useEffect(() => {
		if (voiceMode.isActive && !voiceActivationRef.current) {
			const lastAssistant = [...messages]
				.reverse()
				.find((message) => message.info.role === "assistant");
			if (lastAssistant) {
				const fullText = getMessageText(lastAssistant.parts);
				ttsStreamStateRef.current = {
					messageId: lastAssistant.info.id,
					streamId: null,
					sentLength: fullText.length,
				};
			} else {
				ttsStreamStateRef.current = {
					messageId: null,
					streamId: null,
					sentLength: 0,
				};
			}
		}
		voiceActivationRef.current = voiceMode.isActive;
	}, [voiceMode.isActive, messages]);

	// Auto-TTS: Stream assistant responses to TTS using streaming mode
	// Kokorox handles sentence segmentation and seamless audio playback
	useEffect(() => {
		// Only trigger TTS when voice mode is active and not muted
		if (!voiceMode.isActive || voiceMode.settings.muted) return;

		// Find the last assistant message
		const lastMessage = messages[messages.length - 1];
		if (!lastMessage || lastMessage.info.role !== "assistant") return;

		const messageId = lastMessage.info.id;
		const streamState = ttsStreamStateRef.current;

		// New message - start a new stream
		if (streamState.messageId !== messageId) {
			// End previous stream if any
			if (streamState.streamId) {
				voiceMode.streamEnd();
			}
			streamState.messageId = messageId;
			streamState.streamId = null;
			streamState.sentLength = 0;

			// Start new stream
			voiceMode
				.streamStart()
				.then((streamId) => {
					streamState.streamId = streamId;
					// Send any text that arrived while starting
					const fullText = getMessageText(lastMessage.parts);
					if (fullText && fullText.length > streamState.sentLength) {
						const newText = fullText.slice(streamState.sentLength);
						streamState.sentLength = fullText.length;
						voiceMode.streamAppend(newText);
					}
				})
				.catch((err) => {
					console.error("[Voice] Failed to start TTS stream:", err);
				});
			return;
		}

		// Existing stream - append new text
		const fullText = getMessageText(lastMessage.parts);
		if (!fullText) return;

		// Nothing new to send
		if (fullText.length <= streamState.sentLength) return;

		// Get the new text since last send
		const newText = fullText.slice(streamState.sentLength);
		streamState.sentLength = fullText.length;

		// If stream is ready, append; otherwise it will be sent when stream starts
		if (streamState.streamId) {
			voiceMode.streamAppend(newText);
		}
	}, [
		messages,
		voiceMode.isActive,
		voiceMode.settings.muted,
		voiceMode.streamStart,
		voiceMode.streamAppend,
		voiceMode.streamEnd,
	]);

	// End TTS stream when message finishes (chatState goes from sending to idle)
	const prevChatStateRef = useRef<"idle" | "sending">("idle");
	useEffect(() => {
		const wasStreaming = prevChatStateRef.current === "sending";
		const isNowIdle = chatState === "idle";
		prevChatStateRef.current = chatState;

		// When streaming ends, close the TTS stream to flush remaining text
		if (wasStreaming && isNowIdle && ttsStreamStateRef.current.streamId) {
			voiceMode.streamEnd();
			ttsStreamStateRef.current.streamId = null;
		}
	}, [chatState, voiceMode.streamEnd]);

	// Stop TTS playback when voice mode is deactivated
	useEffect(() => {
		if (!voiceMode.isActive) {
			voiceMode.streamCancel();
			voiceMode.interrupt();
			// Reset stream state so next activation starts fresh
			ttsStreamStateRef.current = {
				messageId: null,
				streamId: null,
				sentLength: 0,
			};
		}
	}, [voiceMode.isActive, voiceMode.interrupt, voiceMode.streamCancel]);

	// Auto-switch to voice tab on desktop when voice mode starts
	useEffect(() => {
		if (voiceMode.isActive && !prevVoiceActiveRef.current && !isMobileLayout) {
			// Voice mode just activated on desktop - switch to voice tab
			setActiveView("voice");
		} else if (
			!voiceMode.isActive &&
			prevVoiceActiveRef.current &&
			activeView === "voice"
		) {
			// Voice mode just deactivated - switch back to tasks
			setActiveView("tasks");
		}
		prevVoiceActiveRef.current = voiceMode.isActive;
	}, [voiceMode.isActive, isMobileLayout, activeView]);

	// Handle mobile keyboard - scroll input into view when keyboard appears
	// iOS Safari requires special handling as it resizes the visual viewport
	useEffect(() => {
		if (typeof window === "undefined") return;

		const viewport = window.visualViewport;
		if (!viewport) return;

		let lastHeight = viewport.height;

		const handleResize = () => {
			const currentHeight = viewport.height;
			const heightDiff = lastHeight - currentHeight;

			// Keyboard likely opened (significant height reduction)
			if (heightDiff > 100) {
				// Scroll the focused input into view
				const activeElement = document.activeElement as HTMLElement;
				if (
					activeElement?.tagName === "INPUT" ||
					activeElement?.tagName === "TEXTAREA"
				) {
					setTimeout(() => {
						activeElement.scrollIntoView({
							behavior: "smooth",
							block: "center",
						});
					}, 100);
				}
			}

			lastHeight = currentHeight;
		};

		viewport.addEventListener("resize", handleResize);
		return () => viewport.removeEventListener("resize", handleResize);
	}, []);

	// Handler for previewing a file from FileTreeView
	const handlePreviewFile = useCallback((filePath: string) => {
		setPreviewFilePath(filePath);
		setActiveView("files");
	}, []);

	const closePreview = useCallback(() => {
		setPreviewFilePath(null);
		setExpandedView((prev) => (prev === "preview" ? null : prev));
	}, []);

	const toggleExpandedView = useCallback(
		(view: Exclude<ExpandedView, null>) => {
			if (isMobileLayout) return;
			if (view === "preview" && !previewFilePath) return;
			if (view === "memories" && !features.mmry_enabled) return;
			setExpandedView((prev) => {
				const next = prev === view ? null : view;
				if (next) {
					setRightSidebarCollapsed(false);
				}
				return next;
			});
		},
		[isMobileLayout, previewFilePath, features.mmry_enabled],
	);

	useEffect(() => {
		if (!previewFilePath && expandedView === "preview") {
			setExpandedView(null);
		}
	}, [previewFilePath, expandedView]);

	const lastPreviewSessionKeyRef = useRef<string | null>(null);
	useEffect(() => {
		const nextKey = mainChatActive ? "main" : selectedChatSessionId || "none";
		if (lastPreviewSessionKeyRef.current === null) {
			lastPreviewSessionKeyRef.current = nextKey;
			return;
		}
		if (lastPreviewSessionKeyRef.current !== nextKey) {
			lastPreviewSessionKeyRef.current = nextKey;
			if (previewFilePath) {
				setPreviewFilePath(null);
			}
		}
	}, [mainChatActive, previewFilePath, selectedChatSessionId]);

	// Handler for opening a file in canvas from FileTreeView
	const handleOpenInCanvas = useCallback((filePath: string) => {
		console.log("[DEBUG] handleOpenInCanvas called with:", filePath);
		setPreviewFilePath(filePath);
		setActiveView("canvas");
	}, []);

	// Handler for file tree state changes (for persistence)
	const handleFileTreeStateChange = useCallback((newState: FileTreeState) => {
		setFileTreeState(newState);
	}, []);

	// Fetch available agents and check workspace config for default agent
	useEffect(() => {
		if (!opencodeBaseUrl || !selectedWorkspaceSessionId) return;

		const loadAgentConfig = async () => {
			try {
				// First, check if workspace has a custom agent in opencode.json
				const workspaceConfig = await getWorkspaceConfig(
					selectedWorkspaceSessionId,
				);

				if (workspaceConfig?.agent) {
					// Workspace specifies a custom agent - use it
					console.log(
						"Using workspace-specified agent:",
						workspaceConfig.agent,
					);
					setDefaultAgent(workspaceConfig.agent);
					return;
				}

				// No workspace config - fetch available agents and default to "build"
				const agents = await fetchAgents(
					opencodeBaseUrl,
					opencodeRequestOptions,
				);
				console.log("Available agents:", agents);

				// Prefer "build" agent (main agent with all tools), fallback to first primary agent
				const buildAgent = agents.find((a) => a.id === "build");
				const firstPrimaryAgent =
					agents.find((a) => a.id === "build" || a.id === "plan") || agents[0];

				if (buildAgent) {
					setDefaultAgent(buildAgent.id);
				} else if (firstPrimaryAgent) {
					setDefaultAgent(firstPrimaryAgent.id);
				}
				// Keep "build" as fallback if no agents found
			} catch (err) {
				console.error("Failed to load agent config:", err);
				// Keep "build" as fallback on error
			}
		};

		loadAgentConfig();
	}, [opencodeBaseUrl, opencodeRequestOptions, selectedWorkspaceSessionId]);

	// Loading state management with timeout
	useEffect(() => {
		// Reset loading state when workspace sessions change
		if (workspaceSessions.length > 0) {
			setIsLoading(false);
			setShowTimeoutError(false);
		} else {
			setIsLoading(true);
			// Show error message after 10 seconds of no response
			const timeout = setTimeout(() => {
				setShowTimeoutError(true);
			}, 10000);
			return () => clearTimeout(timeout);
		}
	}, [workspaceSessions]);

	// File upload handler
	const handleFileUpload = useCallback(
		async (files: FileList | null) => {
			if (!files || files.length === 0) return;
			const workspacePath =
				selectedChatFromHistory?.workspace_path ??
				selectedWorkspaceSession?.workspace_path;
			if (!workspacePath) {
				setStatus(
					locale === "de"
						? "Upload fehlgeschlagen: Kein Workspace gefunden"
						: "Upload failed: no workspace found",
				);
				return;
			}

			setIsUploading(true);
			const uploadedFiles: { name: string; path: string }[] = [];
			const failedFiles: string[] = [];

			try {
				const baseUrl = fileserverWorkspaceBaseUrl();

				for (const file of Array.from(files)) {
					try {
						const destPath = `uploads/${file.name}`;
						const url = new URL(`${baseUrl}/file`, window.location.origin);
						url.searchParams.set("path", destPath);
						url.searchParams.set("mkdir", "true");
						url.searchParams.set("workspace_path", workspacePath);

						const formData = new FormData();
						formData.append("file", file);

						const res = await fetch(url.toString(), {
							method: "POST",
							credentials: "include",
							body: formData,
						});

						if (!res.ok) {
							const text = await res.text().catch(() => res.statusText);
							throw new Error(text || `Upload failed (${res.status})`);
						}

						uploadedFiles.push({ name: file.name, path: destPath });
					} catch (err) {
						const message =
							err instanceof Error ? err.message : "Upload failed";
						console.warn("Upload failed:", file.name, message);
						failedFiles.push(file.name);
					}
				}

				if (uploadedFiles.length > 0) {
					setPendingUploads((prev) => [...prev, ...uploadedFiles]);
				}
				if (failedFiles.length > 0) {
					setStatus(
						locale === "de"
							? `Upload fehlgeschlagen: ${failedFiles.join(", ")}`
							: `Upload failed: ${failedFiles.join(", ")}`,
					);
				}
			} catch (err) {
				setStatus(err instanceof Error ? err.message : "Upload failed");
			} finally {
				setIsUploading(false);
				// Reset file input
				if (fileInputRef.current) {
					fileInputRef.current.value = "";
				}
			}
		},
		[locale, selectedChatFromHistory, selectedWorkspaceSession],
	);

	const removePendingUpload = useCallback((path: string) => {
		setPendingUploads((prev) => prev.filter((u) => u.path !== path));
	}, []);

	// Permission response handler
	const handlePermissionResponse = useCallback(
		async (permissionId: string, response: PermissionResponse) => {
			if (!opencodeBaseUrl || !selectedChatSessionId) {
				throw new Error("No active session");
			}
			await respondToPermission(
				opencodeBaseUrl,
				selectedChatSessionId,
				permissionId,
				response,
				opencodeRequestOptions,
			);
			// Remove from pending list
			setPendingPermissions((prev) =>
				prev.filter((p) => p.id !== permissionId),
			);
		},
		[opencodeBaseUrl, opencodeRequestOptions, selectedChatSessionId],
	);

	// Show next permission when current one is dismissed
	const handlePermissionDismiss = useCallback(() => {
		setActivePermission((current) => {
			// Find next pending permission that isn't the current one
			const next = pendingPermissions.find((p) => p.id !== current?.id);
			return next || null;
		});
	}, [pendingPermissions]);

	// Open permission dialog when clicking the banner
	const handlePermissionBannerClick = useCallback(() => {
		if (pendingPermissions.length > 0) {
			setActivePermission(pendingPermissions[0]);
		}
	}, [pendingPermissions]);

	// Question response handler
	const handleQuestionReply = useCallback(
		async (requestId: string, answers: QuestionAnswer[]) => {
			if (!effectiveOpencodeBaseUrl) {
				throw new Error("No opencode connection");
			}
			await replyToQuestion(
				effectiveOpencodeBaseUrl,
				requestId,
				answers,
				opencodeRequestOptions,
			);
			// Remove from pending list
			setPendingQuestions((prev) => prev.filter((q) => q.id !== requestId));
		},
		[effectiveOpencodeBaseUrl, opencodeRequestOptions],
	);

	// Question reject handler
	const handleQuestionReject = useCallback(
		async (requestId: string) => {
			if (!effectiveOpencodeBaseUrl) {
				throw new Error("No opencode connection");
			}
			await rejectQuestion(
				effectiveOpencodeBaseUrl,
				requestId,
				opencodeRequestOptions,
			);
			// Remove from pending list
			setPendingQuestions((prev) => prev.filter((q) => q.id !== requestId));
		},
		[effectiveOpencodeBaseUrl, opencodeRequestOptions],
	);

	// Show next question when current one is dismissed
	const handleQuestionDismiss = useCallback(() => {
		setActiveQuestion((current) => {
			const next = pendingQuestions.find((q) => q.id !== current?.id);
			return next || null;
		});
	}, [pendingQuestions]);

	// Open question dialog when clicking the banner
	const handleQuestionBannerClick = useCallback(() => {
		if (pendingQuestions.length > 0) {
			setActiveQuestion(pendingQuestions[0]);
		}
	}, [pendingQuestions]);

	const copy = useMemo(
		() => ({
			de: {
				title: "CHAT",
				sessionLabel: "Session",
				refresh: "Aktualisieren",
				noMessages: "Noch keine Nachrichten.",
				inputPlaceholder: "Nachricht eingeben...",
				send: "Senden",
				chat: "Chat",
				files: "Dateien",
				terminal: "Terminal",
				preview: "Vorschau",
				tasks: "Aufgaben",
				memories: "Erinnerungen",
				noSessions: "Keine Sessions verfugbar",
				statusPrefix: "Aktualisiert",
				configNotice: "Control Plane Backend starten, um Sessions zu laden.",
				noTasks: "Keine Aufgaben vorhanden.",
			},
			en: {
				title: "CHAT",
				sessionLabel: "Session",
				refresh: "Refresh",
				noMessages: "No messages yet.",
				inputPlaceholder: "Type a message...",
				send: "Send",
				chat: "Chat",
				files: "Files",
				terminal: "Terminal",
				preview: "Preview",
				tasks: "Tasks",
				memories: "Memories",
				noSessions: "No sessions available",
				statusPrefix: "Updated",
				configNotice: "Start the control plane backend to load sessions.",
				noTasks: "No tasks yet.",
			},
		}),
		[],
	);
	const t = copy[locale];
	const viewLoadingFallback = useMemo(
		() => (
			<div className="flex h-full items-center justify-center text-xs text-muted-foreground">
				{locale === "de" ? "Lade..." : "Loading..."}
			</div>
		),
		[locale],
	);

	const resumeWorkspacePath = mainChatActive
		? (mainChatWorkspacePath ?? undefined)
		: (selectedChatFromHistory?.workspace_path ??
			selectedWorkspaceSession?.workspace_path);
	const canResumeWithoutMessage = useMemo(() => {
		if (!selectedChatSessionId) return false;
		if (!resumeWorkspacePath) return false;
		if (deferredMessageInput.trim()) return false;
		if (pendingUploads.length > 0) return false;
		if (fileAttachments.length > 0) return false;
		if (issueAttachments.length > 0) return false;
		return !opencodeBaseUrl;
	}, [
		deferredMessageInput,
		opencodeBaseUrl,
		pendingUploads.length,
		fileAttachments.length,
		issueAttachments.length,
		resumeWorkspacePath,
		selectedChatSessionId,
	]);

	// Determine if we're viewing a history-only session (no running opencode)
	const isHistoryOnlySession = useMemo(() => {
		// If we have a live opencode session with this ID, it's not history-only
		if (
			selectedChatSession &&
			selectedChatSession.id === selectedChatSessionId
		) {
			return false;
		}
		// If we have a running workspace session with an opencode URL, it's not history-only
		if (selectedWorkspaceSession?.status === "running" && opencodeBaseUrl) {
			return false;
		}
		// If we have this session in disk history but no live session, it's history-only
		if (
			selectedChatFromHistory &&
			selectedChatFromHistory.id === selectedChatSessionId
		) {
			return true;
		}
		return false;
	}, [
		selectedChatSession,
		selectedChatFromHistory,
		selectedChatSessionId,
		selectedWorkspaceSession,
		opencodeBaseUrl,
	]);

	const autoAttachMode = features.session_auto_attach ?? "off";
	const autoAttachScan = features.session_auto_attach_scan ?? false;

	// Reset chatState to idle when session becomes history-only (no live connection)
	// This prevents "Agent working..." from showing when there's no SSE to receive idle events
	useEffect(() => {
		if (isHistoryOnlySession && activeSessionId && chatState === "sending") {
			setChatState("idle");
		}
	}, [isHistoryOnlySession, activeSessionId, chatState, setChatState]);

	// Auto-attach to running sessions (or resume) when opening history sessions.
	useEffect(() => {
		if (isWorkspacePiSession) return;
		if (!selectedChatSessionId || !isHistoryOnlySession) return;
		if (autoAttachMode === "off") return;
		if (!selectedChatFromHistory?.workspace_path) return;

		const workspacePath = selectedChatFromHistory.workspace_path;
		const runningSessions = workspaceSessions.filter(
			(session) =>
				session.status === "running" &&
				session.workspace_path === workspacePath,
		);

		if (runningSessions.length > 0) {
			if (!autoAttachScan) {
				const runningSession = runningSessions[0];
				if (selectedWorkspaceSessionId !== runningSession.id) {
					setSelectedWorkspaceSessionId(runningSession.id);
				}
				return;
			}

			const runningSessionIds = runningSessions
				.map((session) => session.id)
				.sort()
				.join(",");
			const lastScan = autoAttachScanAttemptRef.current;
			if (
				lastScan &&
				lastScan.sessionId === selectedChatSessionId &&
				lastScan.workspacePath === workspacePath &&
				lastScan.runningSessionIds === runningSessionIds
			) {
				return;
			}

			autoAttachScanAttemptRef.current = {
				sessionId: selectedChatSessionId,
				workspacePath,
				runningSessionIds,
			};

			let active = true;
			void (async () => {
				let matched: (typeof runningSessions)[number] | null = null;
				for (const candidate of runningSessions) {
					try {
						const sessions = await fetchSessions(
							opencodeProxyBaseUrl(candidate.id),
							{ directory: workspacePath },
						);
						if (
							sessions.some((session) => session.id === selectedChatSessionId)
						) {
							matched = candidate;
							break;
						}
					} catch {
						// Ignore scan failures and fall back to first running session.
					}
				}

				if (!active) return;
				const target = matched ?? runningSessions[0];
				if (selectedWorkspaceSessionId !== target.id) {
					setSelectedWorkspaceSessionId(target.id);
				}
			})();

			return () => {
				active = false;
			};
		}

		if (autoAttachMode !== "resume") return;

		const startingSession = workspaceSessions.find(
			(session) =>
				session.workspace_path === workspacePath &&
				(session.status === "pending" || session.status === "starting"),
		);
		if (startingSession) return;

		const lastAttempt = autoAttachAttemptRef.current;
		if (
			lastAttempt &&
			lastAttempt.sessionId === selectedChatSessionId &&
			lastAttempt.workspacePath === workspacePath &&
			lastAttempt.mode === autoAttachMode
		) {
			return;
		}

		autoAttachAttemptRef.current = {
			sessionId: selectedChatSessionId,
			workspacePath,
			mode: autoAttachMode,
		};

		void ensureOpencodeRunning(workspacePath);
	}, [
		autoAttachMode,
		autoAttachScan,
		ensureOpencodeRunning,
		isWorkspacePiSession,
		isHistoryOnlySession,
		selectedChatFromHistory,
		selectedChatSessionId,
		selectedWorkspaceSessionId,
		setSelectedWorkspaceSessionId,
		workspaceSessions,
	]);

	// Merge messages to prevent flickering - preserves optimistic (temp-*) messages.
	const mergeMessages = useCallback(mergeSessionMessages, []);

	// Load messages for Main Chat threaded view (all sessions combined)
	const loadMainChatThreadedMessages = useCallback(async () => {
		if (!mainChatAssistantName) return [];
		try {
			return await fetchMainChatThreadedMessages(mainChatAssistantName);
		} catch (err) {
			console.error("Failed to load Main Chat threaded messages:", err);
			return [];
		}
	}, [mainChatAssistantName]);

	const loadMessages = useCallback(
		async (options?: { forceFresh?: boolean }) => {
			// Main Chat Pi view handles its own messages via usePiChat - skip loading here
			if (mainChatActive || isWorkspacePiSession) {
				loadingSessionIdRef.current = "main-chat";
				// Don't load messages - MainChatPiView has its own cached message loading
				return;
			}

			if (!selectedChatSessionId) return;

			// Capture session ID and request counter at start to detect stale responses
			const targetSessionId = selectedChatSessionId;
			loadingSessionIdRef.current = targetSessionId;
			const requestId = ++loadRequestCounterRef.current;
			setMessagesLoading(true);

			try {
				let loadedMessages: OpenCodeMessageWithParts[] = [];

				// Only fetch from opencode if we have a valid opencode session ID (starts with "ses_")
				// Main Chat sessions (pending-*, pi-*, etc.) should not be sent to opencode
				const isOpencodeSession = targetSessionId.startsWith("ses_");

				if (opencodeBaseUrl && !isHistoryOnlySession && isOpencodeSession) {
					// Live opencode is authoritative for streaming updates.
					try {
						loadedMessages = await fetchMessages(
							opencodeBaseUrl,
							targetSessionId,
							{
								directory: opencodeDirectory,
								skipCache: options?.forceFresh,
							},
						);
					} catch (err) {
						console.warn("Failed to load live messages, falling back:", err);
					}
				}

				if (loadedMessages.length === 0) {
					// History-only view (or no live session): use disk history cache.
					try {
						const historyMessages = await getChatMessages(targetSessionId);
						if (historyMessages.length > 0) {
							loadedMessages = convertChatMessagesToOpenCode(historyMessages);
						}
					} catch {
						// Ignore history failures; we don't have a live fallback here.
					}
				}

				// Check if session changed or newer request started - discard stale response
				if (
					loadingSessionIdRef.current !== targetSessionId ||
					loadRequestCounterRef.current !== requestId
				) {
					return;
				}

				// Track that this session has messages (for skeleton display logic)
				if (loadedMessages.length > 0) {
					sessionsWithMessagesRef.current.add(targetSessionId);
				}

				// Use merge to prevent flickering when updating
				startTransition(() => {
					setMessages((prev) => mergeMessages(prev, loadedMessages));
				});
			} catch (err) {
				setStatus((err as Error).message);
			} finally {
				setMessagesLoading(false);
			}
		},
		[
			mainChatActive,
			isWorkspacePiSession,
			opencodeBaseUrl,
			opencodeDirectory,
			selectedChatSessionId,
			isHistoryOnlySession,
			mergeMessages,
		],
	);

	const [eventsTransportMode, setEventsTransportMode] = useState<
		"sse" | "polling" | "ws" | "reconnecting"
	>(features.websocket_events ? "ws" : "sse");
	const messageRefreshStateRef = useRef<{
		timer: ReturnType<typeof setTimeout> | null;
		inFlight: boolean;
		pending: boolean;
		lastStartAt: number;
	}>({
		timer: null,
		inFlight: false,
		pending: false,
		lastStartAt: 0,
	});

	const requestMessageRefresh = useCallback(
		(maxFrequencyMs: number) => {
			const state = messageRefreshStateRef.current;
			state.pending = true;

			if (state.inFlight) return;

			const run = async () => {
				const current = messageRefreshStateRef.current;
				if (current.timer) {
					clearTimeout(current.timer);
					current.timer = null;
				}
				if (current.inFlight || !current.pending) return;

				current.pending = false;
				current.inFlight = true;
				current.lastStartAt = Date.now();
				try {
					await loadMessages({ forceFresh: true });
				} finally {
					current.inFlight = false;
					if (current.pending) requestMessageRefresh(maxFrequencyMs);
				}
			};

			const elapsed = Date.now() - state.lastStartAt;
			const wait = Math.max(0, maxFrequencyMs - elapsed);

			if (wait === 0) {
				void run();
				return;
			}

			if (!state.timer) {
				state.timer = setTimeout(() => void run(), wait);
			}
		},
		[loadMessages],
	);

	const scrollToBottom = useCallback((behavior: ScrollBehavior = "smooth") => {
		const container = messagesContainerRef.current;
		if (!container) return;

		container.scrollTo({
			top: container.scrollHeight,
			behavior,
		});
		autoScrollEnabledRef.current = true;
		setShowScrollToBottom(false);
	}, []);

	// Force auto-scroll while voice mode is active (even if user previously scrolled up)
	useEffect(() => {
		if (!voiceMode.isActive) return;

		autoScrollEnabledRef.current = true;
		setShowScrollToBottom(false);
		pendingVoiceScrollRef.current = true;

		if (selectedChatSessionId) {
			scrollPositionCacheRef.current.set(selectedChatSessionId, null);
		}

		if (messagesContainerRef.current) {
			scrollToBottom("auto");
			pendingVoiceScrollRef.current = false;
		}
	}, [voiceMode.isActive, selectedChatSessionId, scrollToBottom]);

	// Apply pending voice-mode auto-scroll when returning to chat view
	useEffect(() => {
		if (activeView !== "chat") return;
		if (!pendingVoiceScrollRef.current) return;
		scrollToBottom("auto");
		pendingVoiceScrollRef.current = false;
	}, [activeView, scrollToBottom]);

	useEffect(() => {
		loadMessages();
	}, [loadMessages]);

	// RAF-throttled scroll handler state
	const scrollRafRef = useRef<number | null>(null);
	const pendingScrollRef = useRef(false);

	// Handle scroll events to show/hide scroll to bottom button and cache position
	// Throttled via RAF to avoid blocking the main thread
	const handleScroll = useCallback(() => {
		pendingScrollRef.current = true;

		// If RAF already scheduled, let it handle the update
		if (scrollRafRef.current !== null) return;

		scrollRafRef.current = requestAnimationFrame(() => {
			scrollRafRef.current = null;
			if (!pendingScrollRef.current) return;
			pendingScrollRef.current = false;

			const container = messagesContainerRef.current;
			if (!container) return;

			const { scrollTop, scrollHeight, clientHeight } = container;
			const distanceFromBottom = scrollHeight - scrollTop - clientHeight;
			const lastScrollTop = lastScrollTopRef.current;
			lastScrollTopRef.current = scrollTop;

			// Detect if user scrolled up (intentionally moving away from bottom)
			const scrolledUp = scrollTop < lastScrollTop;
			const isAtBottom = distanceFromBottom < 50;

			// Disable auto-scroll when user scrolls up away from bottom
			if (scrolledUp && distanceFromBottom > 100) {
				autoScrollEnabledRef.current = false;
			}

			// Re-enable auto-scroll when user scrolls to bottom
			if (isAtBottom) {
				autoScrollEnabledRef.current = true;
			}

			// Cache scroll position for current session
			if (selectedChatSessionId) {
				if (isAtBottom) {
					// At bottom - clear cached position so next load scrolls to bottom
					scrollPositionCacheRef.current.set(selectedChatSessionId, null);
				} else {
					// Save scroll position
					scrollPositionCacheRef.current.set(selectedChatSessionId, scrollTop);
				}
			}

			// Only update state if threshold actually changed
			const shouldShow = distanceFromBottom > 100;
			setShowScrollToBottom((prev) => {
				if (prev === shouldShow) return prev;
				return shouldShow;
			});
		});
	}, [selectedChatSessionId]);

	const messageCount = messages.length;

	// Check scroll position when messages change or content might have resized
	useEffect(() => {
		if (messageCount === 0) {
			setShowScrollToBottom(false);
			return;
		}
		// Use RAF to wait for DOM update before checking scroll position
		requestAnimationFrame(() => {
			handleScroll();
		});
	}, [messageCount, handleScroll]);

	// Reset state when switching sessions
	useEffect(() => {
		if (!selectedChatSessionId) return;

		if (lastSessionIdRef.current !== selectedChatSessionId) {
			lastSessionIdRef.current = selectedChatSessionId;
			// Check if we have a cached scroll position for this session
			const cachedPosition = scrollPositionCacheRef.current.get(
				selectedChatSessionId,
			);
			// Enable auto-scroll only if no cached position (user was at bottom)
			autoScrollEnabledRef.current =
				cachedPosition === null || cachedPosition === undefined;
			initialLoadRef.current = true;
		}
	}, [selectedChatSessionId]);

	// Position at cached position or bottom synchronously before paint (no visible jump)
	useLayoutEffect(() => {
		if (messages.length === 0) return;

		const container = messagesContainerRef.current;
		if (!container) return;

		if (initialLoadRef.current && selectedChatSessionId) {
			const cachedPosition = scrollPositionCacheRef.current.get(
				selectedChatSessionId,
			);
			if (cachedPosition !== null && cachedPosition !== undefined) {
				// Restore user's scroll position instantly
				container.scrollTop = cachedPosition;
			} else {
				// Scroll to bottom instantly (no animation)
				container.scrollTop = container.scrollHeight;
			}
			initialLoadRef.current = false;
		}
	}, [messages, selectedChatSessionId]);

	// Smooth scroll for new messages during conversation (after initial load)
	useEffect(() => {
		if (messages.length === 0) return;
		if (!autoScrollEnabledRef.current) return;
		if (initialLoadRef.current) return; // Skip - handled by useLayoutEffect

		// Use RAF to ensure DOM has updated before scrolling
		requestAnimationFrame(() => {
			if (autoScrollEnabledRef.current) {
				scrollToBottom("smooth");
			}
		});
	}, [messages, scrollToBottom]);

	// Scroll to message when scrollToMessageId changes (from search results)
	useEffect(() => {
		if (!scrollToMessageId || !messagesContainerRef.current) return;

		let targetId = scrollToMessageId;
		if (targetId.startsWith("line-")) {
			const idx = Number.parseInt(targetId.slice(5), 10);
			const resolved = Number.isFinite(idx) ? messages[idx - 1]?.id : undefined;
			if (resolved) targetId = resolved;
		}

		// Find the message element with this ID
		const messageEl = messagesContainerRef.current.querySelector(
			`[data-message-id="${targetId}"]`,
		);

		if (messageEl) {
			// Disable auto-scroll to prevent jumping back to bottom
			autoScrollEnabledRef.current = false;

			// Scroll to the message
			requestAnimationFrame(() => {
				messageEl.scrollIntoView({ behavior: "auto", block: "center" });
				// Add highlight animation
				messageEl.classList.add("search-highlight");
				setTimeout(() => {
					messageEl.classList.remove("search-highlight");
				}, 2000);
			});

			// Clear the scroll target
			setScrollToMessageId(null);
		}
	}, [scrollToMessageId, setScrollToMessageId, messages]);

	// Keyboard shortcut for search (Ctrl+F / Cmd+F)
	useEffect(() => {
		const handleKeyDown = (e: KeyboardEvent) => {
			if ((e.ctrlKey || e.metaKey) && e.key === "f") {
				e.preventDefault();
				setIsSearchOpen(true);
			}
			if (e.key === "Escape" && isSearchOpen) {
				setIsSearchOpen(false);
			}
		};
		window.addEventListener("keydown", handleKeyDown);
		return () => window.removeEventListener("keydown", handleKeyDown);
	}, [isSearchOpen]);

	// Handle search result selection - scroll to message by messageId or line number
	const handleSearchResult = useCallback(
		(result: { lineNumber: number; messageId?: string }) => {
			const container = messagesContainerRef.current;
			if (!container) return;

			// Try to find target message - prefer messageId if available
			let targetMessageId: string | undefined;
			if (result.messageId) {
				// Direct message ID from search result
				targetMessageId = result.messageId;
			} else {
				// Fallback: estimate from line number (legacy behavior)
				const messageIndex = Math.max(0, result.lineNumber - 2);
				if (messageIndex < messages.length) {
					targetMessageId = messages[messageIndex].info.id;
				}
			}

			if (!targetMessageId) return;

			// Scroll to message
			requestAnimationFrame(() => {
				const messageEl = container.querySelector(
					`[data-message-id="${targetMessageId}"]`,
				);
				if (messageEl) {
					autoScrollEnabledRef.current = false;
					messageEl.scrollIntoView({ behavior: "smooth", block: "center" });
					messageEl.classList.add("search-highlight");
					setTimeout(() => {
						messageEl.classList.remove("search-highlight");
					}, 2000);
				}
			});
		},
		[messages],
	);

	// Event handler for session events (WebSocket-only)
	const handleSessionEvent = useCallback(
		(event: WsEvent) => {
			const eventType = event.type as string;

			// Debug: log events only when perf debugging is enabled
			if (
				perfEnabled &&
				eventType !== "message_updated" &&
				eventType !== "text_delta" &&
				eventType !== "thinking_delta"
			) {
				console.log("[Event]", eventType, event);
			}

			if (eventType === "connected" || eventType === "agent_connected") {
				startTransition(() => {
					if (effectiveOpencodeBaseUrl && activeSessionId) {
						invalidateMessageCache(
							effectiveOpencodeBaseUrl,
							activeSessionId,
							opencodeDirectory,
						);
						requestMessageRefresh(250);
					}
				});
			}

			if (eventType === "agent_disconnected") {
				const now = Date.now();
				const lastAttempt = sessionUnavailableRef.current;
				if (
					lastAttempt?.sessionId === selectedChatSessionId &&
					now - lastAttempt.attemptedAt < 15_000
				) {
					return;
				}
				sessionUnavailableRef.current = {
					sessionId: selectedChatSessionId ?? "",
					attemptedAt: now,
				};

				const resumePath =
					selectedChatFromHistory?.workspace_path ?? resumeWorkspacePath;

				// Extract disconnect reason if available
				const disconnectReason =
					"reason" in event && typeof event.reason === "string"
						? event.reason
						: undefined;

				// Check if this looks like a crash (contains exit code or signal info)
				const isCrash =
					disconnectReason &&
					(disconnectReason.includes("exited") ||
						disconnectReason.includes("signal") ||
						disconnectReason.includes("killed"));

				const title = isCrash
					? locale === "de"
						? "Sitzung abgestuerzt"
						: "Session crashed"
					: locale === "de"
						? "Sitzung getrennt"
						: "Session disconnected";

				const description = disconnectReason
					? disconnectReason
					: locale === "de"
						? "Verbindung zum Agenten verloren."
						: "Lost connection to the agent.";

				console.error(
					"[Session Disconnected]",
					disconnectReason || "no reason",
				);

				toast.error(title, {
					description,
					action: resumePath
						? {
								label: locale === "de" ? "Neu verbinden" : "Reconnect",
								onClick: () => {
									void ensureOpencodeRunning(resumePath);
								},
							}
						: undefined,
					duration: 10_000,
				});

				if (autoAttachMode === "resume" && resumePath) {
					void ensureOpencodeRunning(resumePath).then((url) => {
						if (!url) {
							toast.error(
								locale === "de"
									? "Wiederherstellen fehlgeschlagen"
									: "Failed to resume session",
							);
						}
					});
				}
			}

			if (eventType === "session_idle") {
				setChatState("idle");
				startTransition(() => {
					if (effectiveOpencodeBaseUrl && activeSessionId) {
						invalidateMessageCache(
							effectiveOpencodeBaseUrl,
							activeSessionId,
							opencodeDirectory,
						);
					}
					loadMessages();
					refreshOpencodeSessions();
					refreshChatHistory();
				});
			} else if (eventType === "session_busy") {
				setChatState("sending");
				if (
					event.session_id === selectedChatSessionId ||
					event.session_id === mainChatCurrentSessionId
				) {
					lastActiveChatSessionRef.current = event.session_id;
				}
			}

			if (eventType === "permission_request") {
				const permission = normalizePermissionEvent(event);
				if (!permission) return;
				console.log("[Permission] Received permission request:", permission);
				setPendingPermissions((prev) => {
					if (prev.some((p) => p.id === permission.id)) return prev;
					return [...prev, permission];
				});
				setActivePermission((current) => current || permission);
			} else if (eventType === "permission_resolved") {
				const permissionID =
					"permission_id" in event ? event.permission_id : "";
				if (!permissionID) return;
				console.log("[Permission] Permission replied:", permissionID);
				setPendingPermissions((prev) =>
					prev.filter((p) => p.id !== permissionID),
				);
				setActivePermission((current) =>
					current?.id === permissionID ? null : current,
				);
			}

			if (eventType === "question_request") {
				if (!("request_id" in event) || !("questions" in event)) return;
				console.log("[Question] Received question request:", event);
				setPendingQuestions((prev) => {
					if (prev.some((q) => q.id === event.request_id)) return prev;
					return [
						...prev,
						{
							id: event.request_id,
							questions: event.questions as QuestionRequest["questions"],
							tool: event.tool,
						},
					];
				});
				setActiveQuestion(
					(current) =>
						current || {
							id: event.request_id,
							questions: event.questions as QuestionRequest["questions"],
							tool: event.tool,
						},
				);
			} else if (eventType === "question_resolved") {
				const requestID = "request_id" in event ? event.request_id : "";
				if (!requestID) return;
				console.log("[Question] Question resolved:", requestID);
				setPendingQuestions((prev) => prev.filter((q) => q.id !== requestID));
				setActiveQuestion((current) =>
					current?.id === requestID ? null : current,
				);
			}

			if (eventType === "session_error" || eventType === "error") {
				const errorName =
					eventType === "session_error" && "error_type" in event
						? event.error_type
						: "Error";
				const errorMessage =
					eventType === "session_error" && "message" in event
						? event.message
						: "message" in event
							? event.message
							: "An unknown error occurred";
				console.error("[Session Error]", errorName, errorMessage);
				toast.error(errorMessage, {
					description: errorName !== "UnknownError" ? errorName : undefined,
					duration: 8000,
				});
				setChatState("idle");
			}

			if (eventType === "tool_end" && "is_error" in event && event.is_error) {
				const message =
					typeof event.result === "string"
						? event.result
						: "Tool execution failed";
				toast.error(message, { duration: 8000 });
				setChatState("idle");
			}

			if (eventType === "compaction_end") {
				if (!("success" in event) || event.success !== false) {
					startTransition(() => {
						setLastCompactionAt(Date.now());
					});
				}
			}

			if (
				eventType === "message_updated" ||
				eventType === "message_start" ||
				eventType === "message_end" ||
				eventType === "text_delta" ||
				eventType === "thinking_delta" ||
				eventType === "tool_start" ||
				eventType === "tool_end"
			) {
				// Only invalidate cache on completion events to reduce overhead
				// High-frequency events (text_delta, thinking_delta) just trigger refresh
				const isCompletionEvent =
					eventType === "message_end" || eventType === "tool_end";
				const isHighFrequency =
					eventType === "text_delta" || eventType === "thinking_delta";

				startTransition(() => {
					if (
						isCompletionEvent &&
						effectiveOpencodeBaseUrl &&
						activeSessionId
					) {
						invalidateMessageCache(
							effectiveOpencodeBaseUrl,
							activeSessionId,
							opencodeDirectory,
						);
					}
					// Use longer throttle for high-frequency events
					requestMessageRefresh(isHighFrequency ? 500 : 1000);
				});
			}

			if (eventType === "a2ui_surface") {
				if (!("surface_id" in event) || !("messages" in event)) return;
				console.log("[A2UI] Surface received:", event.surface_id, event);
				setA2uiSurfaces((prev) => {
					const existing = prev.findIndex(
						(s) => s.surfaceId === event.surface_id,
					);
					const newSurface: A2UISurface = {
						surfaceId: event.surface_id,
						sessionId: event.session_id,
						messages: event.messages as A2UIMessage[],
						blocking: event.blocking ?? false,
						requestId: event.request_id,
					};
					if (existing >= 0) {
						const updated = [...prev];
						updated[existing] = newSurface;
						return updated;
					}
					return [...prev, newSurface];
				});
			}

			if (eventType === "a2ui_action_resolved") {
				if ("request_id" in event) {
					console.log("[A2UI] Action resolved:", event.request_id);
					setA2uiSurfaces((prev) =>
						prev.filter((s) => s.requestId !== event.request_id),
					);
				}
			}
		},
		[
			autoAttachMode,
			ensureOpencodeRunning,
			effectiveOpencodeBaseUrl,
			opencodeDirectory,
			activeSessionId,
			selectedChatSessionId,
			mainChatCurrentSessionId,
			selectedChatFromHistory,
			resumeWorkspacePath,
			locale,
			loadMessages,
			refreshOpencodeSessions,
			refreshChatHistory,
			requestMessageRefresh,
			setChatState,
			perfEnabled,
		],
	);

	// Subscribe to session events (uses WebSocket when enabled, SSE otherwise)
	// Disabled when Main Chat is active - MainChatPiView handles its own events
	const { transportMode: sessionTransportMode } = useSessionEvents(
		handleSessionEvent,
		{
			useWebSocket: true,
			workspaceSessionId: selectedWorkspaceSessionId,
			enabled:
				!mainChatActive && !!effectiveOpencodeBaseUrl && !!activeSessionId,
		},
	);

	// Sync transport mode from hook
	useEffect(() => {
		setEventsTransportMode(sessionTransportMode);
	}, [sessionTransportMode]);

	// A2UI events are now handled by the useA2UI hook

	// Poll for message updates while assistant is working.
	// This runs regardless of SSE status since SSE is unreliable through the proxy.
	// Disabled when Main Chat is active - MainChatPiView handles its own polling.
	useEffect(() => {
		if (
			mainChatActive ||
			chatState !== "sending" ||
			!effectiveOpencodeBaseUrl ||
			!activeSessionId
		)
			return;

		let active = true;
		let delayMs = 1000;
		let timer: number | null = null;

		const tick = async () => {
			if (!active) return;
			try {
				// Invalidate cache and fetch fresh data
				invalidateMessageCache(
					effectiveOpencodeBaseUrl,
					activeSessionId,
					opencodeDirectory,
				);
				const freshMessages = await fetchMessages(
					effectiveOpencodeBaseUrl,
					activeSessionId,
					{ skipCache: true, directory: opencodeDirectory },
				);
				if (!active) return;

				// Use merge to prevent flickering
				setMessages((prev) => mergeMessages(prev, freshMessages));

				// Check if the latest assistant message is completed
				const lastMessage = freshMessages[freshMessages.length - 1];
				if (lastMessage?.info.role === "assistant") {
					const assistantInfo = lastMessage.info as {
						time?: { completed?: number };
					};
					if (assistantInfo.time?.completed) {
						// Assistant is done, set to idle
						setChatState("idle");
						refreshOpencodeSessions();
						return; // Stop polling
					}
				}
			} catch {
				// Ignore errors, will retry
			}

			if (!active) return;
			delayMs = Math.min(3000, Math.round(delayMs * 1.1));
			timer = window.setTimeout(
				() => void tick(),
				delayMs,
			) as unknown as number;
		};

		// Start polling immediately
		void tick();

		return () => {
			active = false;
			if (timer) window.clearTimeout(timer);
		};
	}, [
		mainChatActive,
		chatState,
		effectiveOpencodeBaseUrl,
		opencodeDirectory,
		activeSessionId,
		refreshOpencodeSessions,
		mergeMessages,
		setChatState,
	]);

	useEffect(() => {
		return () => {
			const state = messageRefreshStateRef.current;
			if (state.timer) {
				clearTimeout(state.timer);
				state.timer = null;
			}
		};
	}, []);

	// Double-Escape keyboard shortcut to stop agent (like opencode TUI)
	useEffect(() => {
		let lastEscapeTime = 0;
		const DOUBLE_PRESS_THRESHOLD = 500; // ms

		const handleKeyDown = (e: KeyboardEvent) => {
			if (e.key === "Escape" && chatState === "sending") {
				const now = Date.now();
				if (now - lastEscapeTime < DOUBLE_PRESS_THRESHOLD) {
					// Double-escape detected - stop the agent
					e.preventDefault();
					if (opencodeBaseUrl && selectedChatSessionId) {
						abortSession(opencodeBaseUrl, selectedChatSessionId, {
							directory: opencodeDirectory,
						})
							.then(() => {
								setChatState("idle");
								setStatus(locale === "de" ? "Abgebrochen" : "Stopped");
								setTimeout(() => setStatus(""), 2000);
							})
							.catch((err) => setStatus((err as Error).message));
					}
					lastEscapeTime = 0; // Reset
				} else {
					lastEscapeTime = now;
				}
			}
		};

		window.addEventListener("keydown", handleKeyDown);
		return () => window.removeEventListener("keydown", handleKeyDown);
	}, [
		chatState,
		opencodeBaseUrl,
		opencodeDirectory,
		selectedChatSessionId,
		locale,
		setChatState,
	]);

	const selectedSession = useMemo(() => {
		if (!selectedWorkspaceSessionId) return undefined;
		return workspaceSessions.find(
			(session) => session.id === selectedWorkspaceSessionId,
		);
	}, [workspaceSessions, selectedWorkspaceSessionId]);

	const messageGroups = useMemo(() => groupMessages(messages), [messages]);

	// Progressive rendering - show last N groups immediately, expand on scroll up
	const [visibleGroupCount, setVisibleGroupCount] = useState(20);

	// Reset visible count and clear messages when session changes
	useEffect(() => {
		setVisibleGroupCount(20);
		setMessages([]); // Clear messages immediately on session switch
		// Invalidate any in-flight loadMessages requests for the old session
		loadingSessionIdRef.current = selectedChatSessionId ?? null;
		// Set loading state for sessions that have had messages before
		if (
			selectedChatSessionId &&
			sessionsWithMessagesRef.current.has(selectedChatSessionId)
		) {
			setMessagesLoading(true);
		}
		if (!selectedChatSessionId) {
			return;
		}
	}, [selectedChatSessionId]);

	// Calculate which groups to show (from the end, so newest messages are visible)
	const visibleGroups = useMemo(() => {
		if (messageGroups.length <= visibleGroupCount) {
			return messageGroups;
		}
		return messageGroups.slice(-visibleGroupCount);
	}, [messageGroups, visibleGroupCount]);

	const hasHiddenMessages = messageGroups.length > visibleGroupCount;

	// Map A2UI surfaces to their corresponding message group
	// A2UI surfaces are attached to the message group containing their anchor message
	const a2uiByGroupIndex = useMemo(() => {
		const map = new Map<number, typeof a2uiSurfaces>();
		for (const surface of a2uiSurfaces) {
			// Find the group containing the anchor message ID
			let targetGroupIndex = -1;
			if (surface.anchorMessageId) {
				for (let i = 0; i < visibleGroups.length; i++) {
					const group = visibleGroups[i];
					if (
						group.messages.some((m) => m.info.id === surface.anchorMessageId)
					) {
						targetGroupIndex = i;
						break;
					}
				}
			}
			// If no anchor or not found, attach to the last assistant group
			if (targetGroupIndex === -1) {
				for (let i = visibleGroups.length - 1; i >= 0; i--) {
					if (visibleGroups[i].role === "assistant") {
						targetGroupIndex = i;
						break;
					}
				}
			}
			// Fallback to last group
			if (targetGroupIndex === -1 && visibleGroups.length > 0) {
				targetGroupIndex = visibleGroups.length - 1;
			}
			if (targetGroupIndex >= 0) {
				const existing = map.get(targetGroupIndex) || [];
				existing.push(surface);
				map.set(targetGroupIndex, existing);
			}
		}
		return map;
	}, [visibleGroups, a2uiSurfaces]);

	const loadMoreMessages = useCallback(() => {
		setVisibleGroupCount((prev) => Math.min(prev + 20, messageGroups.length));
	}, [messageGroups.length]);

	// Calculate total tokens and extract current model for context window gauge
	// Only count tokens from messages AFTER the last compaction (since compaction resets the context)
	const tokenUsage = useMemo(() => {
		let inputTokens = 0;
		let outputTokens = 0;
		let providerID: string | undefined;
		let modelID: string | undefined;

		// Find the index of the last message containing a compaction part
		let lastCompactionIndex = -1;
		for (let i = messages.length - 1; i >= 0; i--) {
			const msg = messages[i];
			if (msg.parts.some((part) => part.type === "compaction")) {
				lastCompactionIndex = i;
				break;
			}
		}

		// Only count tokens from messages after the last compaction
		let startIndex = lastCompactionIndex >= 0 ? lastCompactionIndex + 1 : 0;
		if (lastCompactionAt) {
			const timeIndex = messages.findIndex((msg) => {
				const created = msg.info.time?.created;
				return typeof created === "number" && created >= lastCompactionAt;
			});
			if (timeIndex >= 0) {
				startIndex = Math.max(startIndex, timeIndex);
			} else if (messages.length > 0) {
				const lastCreated = messages[messages.length - 1]?.info.time?.created;
				if (typeof lastCreated === "number" && lastCreated < lastCompactionAt) {
					startIndex = messages.length;
				}
			}
		}

		for (let i = startIndex; i < messages.length; i++) {
			const msg = messages[i];
			if (msg.info.role === "assistant") {
				const assistantInfo = msg.info as OpenCodeAssistantMessage;
				if (assistantInfo.tokens) {
					inputTokens += assistantInfo.tokens.input || 0;
					outputTokens += assistantInfo.tokens.output || 0;
				}
				// Track the most recent model used
				if (assistantInfo.providerID && assistantInfo.modelID) {
					providerID = assistantInfo.providerID;
					modelID = assistantInfo.modelID;
				}
			}
		}

		return { inputTokens, outputTokens, providerID, modelID };
	}, [lastCompactionAt, messages]);

	// Get context limit from models.dev based on current model
	const contextLimit = useModelContextLimit(
		tokenUsage.providerID,
		tokenUsage.modelID,
		200000, // Default fallback
	);
	const displayTokenUsage = useMemo(() => {
		if (mainChatActive) return mainChatTokenUsage;
		if (isWorkspacePiSession) return workspacePiTokenUsage;
		return {
			inputTokens: tokenUsage.inputTokens,
			outputTokens: tokenUsage.outputTokens,
			maxTokens: contextLimit,
		};
	}, [
		contextLimit,
		isWorkspacePiSession,
		mainChatActive,
		mainChatTokenUsage,
		tokenUsage.inputTokens,
		tokenUsage.outputTokens,
		workspacePiTokenUsage,
	]);
	const displayContextLimit = displayTokenUsage.maxTokens || contextLimit;

	useEffect(() => {
		if (mainChatActive) return;
		if (selectedModelRef) return;
		if (tokenUsage.providerID && tokenUsage.modelID) {
			setSelectedModelRef(`${tokenUsage.providerID}/${tokenUsage.modelID}`);
		}
	}, [
		mainChatActive,
		selectedModelRef,
		tokenUsage.providerID,
		tokenUsage.modelID,
	]);

	// Extract the latest todo list from messages (OpenCode sessions)
	const opencodeTodos = useMemo(() => {
		// Go through all messages in reverse to find the most recent todowrite
		for (let i = messages.length - 1; i >= 0; i--) {
			const msg = messages[i];
			for (let j = msg.parts.length - 1; j >= 0; j--) {
				const part = msg.parts[j];
				if (part.type === "tool" && part.tool?.toLowerCase().includes("todo")) {
					const input = part.state?.input as
						| Record<string, unknown>
						| undefined;
					if (input?.todos && Array.isArray(input.todos)) {
						return input.todos as TodoItem[];
					}
				}
			}
		}
		return [];
	}, [messages]);

	// Use Pi todos for main chat/workspace Pi, otherwise use opencode todos
	const latestTodos = mainChatActive
		? mainChatTodos
		: isWorkspacePiSession
			? workspacePiTodos
			: opencodeTodos;

	// Handle slash command selection from popup
	const handleSlashCommandSelect = useCallback(
		async (cmd: SlashCommand) => {
			setShowSlashPopup(false);

			// Send opencode command (e.g., /init, /undo, /redo, or custom commands)
			if (!selectedChatSessionId || !opencodeBaseUrl) return;

			// Set flag to ignore stale onChange events
			ignoringInputRef.current = true;

			// Clear input
			messageInputRef.current = "";
			if (chatInputRef.current) {
				chatInputRef.current.value = "";
				chatInputRef.current.style.height = "36px";
			}
			syncInputToState("");

			// Reset flag after a microtask
			queueMicrotask(() => {
				ignoringInputRef.current = false;
			});

			try {
				// Command name without slash, args separately
				await sendCommandAsync(
					opencodeBaseUrl,
					selectedChatSessionId,
					cmd.name,
					slashQuery.args,
					opencodeRequestOptions,
				);
			} catch (err) {
				console.error("Failed to send command:", err);
				setStatus(
					`Command failed: ${err instanceof Error ? err.message : "Unknown error"}`,
				);
			}
		},
		[
			selectedChatSessionId,
			opencodeBaseUrl,
			opencodeRequestOptions,
			slashQuery.args,
			syncInputToState,
		],
	);

	// Handle canvas save and add to chat
	const handleCanvasSaveAndAddToChat = useCallback((filePath: string) => {
		// Add the saved canvas image as a file attachment
		const attachment: FileAttachment = {
			id: `canvas-${Date.now()}-${Math.random().toString(36).slice(2)}`,
			path: filePath,
			filename: filePath.split("/").pop() || filePath,
			type: "file",
		};
		setFileAttachments((prev) => [...prev, attachment]);
		// Switch to chat view so user can see the attachment and send
		setActiveView("chat");
		// Focus the chat input
		setTimeout(() => {
			chatInputRef.current?.focus();
		}, 100);
	}, []);

	// Handle forking/branching a session from a specific message
	// Creates a new session and copies the conversation context to clipboard for easy pasting
	const handleForkSession = useCallback(
		async (messageId: string) => {
			// We need either an active session OR a way to resume one
			const sessionId = activeSessionId || selectedChatSessionId;
			if (!sessionId) {
				setStatus(
					locale === "de"
						? "Keine Sitzung zum Verzweigen"
						: "No session to fork",
				);
				return;
			}

			try {
				// Determine the base URL - resume session if needed
				let baseUrl = effectiveOpencodeBaseUrl;
				let workspacePath = opencodeDirectory;

				if (!baseUrl && resumeWorkspacePath) {
					// Need to resume the session first
					setStatus(
						locale === "de"
							? "Sitzung wird wiederhergestellt..."
							: "Resuming session...",
					);

					const url = await ensureOpencodeRunning(resumeWorkspacePath);
					if (!url) {
						throw new Error(
							locale === "de"
								? "Sitzung konnte nicht wiederhergestellt werden"
								: "Failed to resume session",
						);
					}
					baseUrl = url;
					workspacePath = resumeWorkspacePath;
				}

				if (!baseUrl) {
					throw new Error(
						locale === "de"
							? "Keine aktive Sitzung zum Verzweigen"
							: "No active session to fork",
					);
				}

				setStatus(
					locale === "de"
						? "Sitzung wird verzweigt..."
						: "Branching session...",
				);

				// Find all messages up to and including the selected message
				const messageIndex = messages.findIndex((m) => m.info.id === messageId);

				if (messageIndex === -1) {
					throw new Error("Message not found");
				}

				// Get messages up to the selected point
				const messagesToCopy = messages.slice(0, messageIndex + 1);

				// Build a concise conversation transcript
				const conversationTranscript = messagesToCopy
					.map((msg) => {
						const role = msg.info.role === "user" ? "User" : "Assistant";
						const textParts = msg.parts
							.filter((p) => p.type === "text" && p.text)
							.map((p) => p.text)
							.join("\n");
						// Truncate long messages to keep context manageable
						const truncated =
							textParts.length > 500
								? `${textParts.substring(0, 500)}...`
								: textParts;
						return `[${role}]: ${truncated}`;
					})
					.join("\n\n");

				// Create a new session with parentID linking to original
				const parentTitle =
					selectedChatSession?.title ||
					selectedChatFromHistory?.title ||
					resolveReadableId(sessionId, session?.readable_id);
				const requestOptions = workspacePath
					? { directory: workspacePath }
					: opencodeRequestOptions;
				const newSession = await createSession(
					baseUrl,
					`Branch: ${parentTitle}`,
					sessionId, // parentID for linking
					requestOptions,
				);

				// Refresh sessions to show the new session
				await refreshOpencodeSessions();
				await refreshChatHistory();

				// Switch to the new session
				if (newSession.id) {
					setSelectedChatSessionId(newSession.id);

					// Copy conversation context to clipboard
					const contextForClipboard =
						locale === "de"
							? `[Kontext aus vorheriger Unterhaltung - bei Bedarf einfügen]\n\n${conversationTranscript}`
							: `[Context from previous conversation - paste if needed]\n\n${conversationTranscript}`;

					try {
						await navigator.clipboard?.writeText(contextForClipboard);
					} catch {
						// Clipboard access may be denied, that's okay
					}

					setStatus(
						locale === "de"
							? "Verzweigt! Kontext in Zwischenablage kopiert."
							: "Branched! Context copied to clipboard.",
					);

					// Focus the input for immediate typing
					setTimeout(() => {
						chatInputRef.current?.focus();
					}, 100);

					// Clear status after a moment
					setTimeout(() => setStatus(""), 4000);
				}
			} catch (err) {
				console.error("Failed to fork session:", err);
				setStatus(
					locale === "de"
						? `Verzweigen fehlgeschlagen: ${err instanceof Error ? err.message : "Unbekannter Fehler"}`
						: `Fork failed: ${err instanceof Error ? err.message : "Unknown error"}`,
				);
			}
		},
		[
			effectiveOpencodeBaseUrl,
			activeSessionId,
			selectedChatSessionId,
			selectedChatSession,
			selectedChatFromHistory,
			messages,
			opencodeDirectory,
			opencodeRequestOptions,
			resumeWorkspacePath,
			ensureOpencodeRunning,
			locale,
			refreshOpencodeSessions,
			refreshChatHistory,
			setSelectedChatSessionId,
		],
	);

	const handleSend = async () => {
		// In Main Chat mode, we might need to create a session first
		// In regular mode, we need a session ID
		if (!mainChatActive && !selectedChatSessionId) return;

		// Use the ref value directly to avoid race conditions with debounced state sync.
		// The user may type and press Enter before the 100ms debounce fires.
		const currentInput = messageInputRef.current.trim();

		if (
			!currentInput &&
			pendingUploads.length === 0 &&
			fileAttachments.length === 0
		)
			return;

		// Stop dictation if active
		if (dictation.isActive) {
			dictation.stop();
		}

		// Close popups if open
		setShowSlashPopup(false);
		setShowFileMentionPopup(false);
		setShowAgentMentionPopup(false);

		// Capture agent target before clearing
		const currentAgentTarget = agentTarget;

		// Capture file and issue attachments before clearing
		const currentFileAttachments = [...fileAttachments];
		const currentIssueAttachments = [...issueAttachments];

		// Build message text with uploaded file paths
		let messageText = currentInput;
		if (pendingUploads.length > 0) {
			const uploadPrefix =
				pendingUploads.length === 1
					? `[Uploaded file: ${pendingUploads[0].path}]`
					: `[Uploaded files: ${pendingUploads.map((u) => u.path).join(", ")}]`;
			messageText = messageText
				? `${uploadPrefix}\n\n${messageText}`
				: uploadPrefix;
		}

		// Build message text with issue attachments
		if (currentIssueAttachments.length > 0) {
			const issueText = currentIssueAttachments
				.map(
					(attachment) =>
						`Working on #${attachment.issueId}: ${attachment.title}${attachment.description ? `\n\n${attachment.description}` : ""}`,
				)
				.join("\n\n---\n\n");
			messageText = messageText ? `${issueText}\n\n${messageText}` : issueText;
		}

		// Check if this is a shell command (starts with "!")
		const isShellCommand = messageText.startsWith("!");
		const shellCommand = isShellCommand ? messageText.slice(1).trim() : "";

		// Cancel any pending draft save and invalidate stale writes
		if (draftSaveTimeoutRef.current) {
			clearTimeout(draftSaveTimeoutRef.current);
			draftSaveTimeoutRef.current = null;
		}
		draftWriteTokenRef.current++;

		// Cancel any pending input state sync to prevent race conditions
		if (inputSyncTimeoutRef.current) {
			clearTimeout(inputSyncTimeoutRef.current);
			inputSyncTimeoutRef.current = null;
		}

		// Set flag to ignore stale onChange events that may fire after clearing
		ignoringInputRef.current = true;

		// Clear input immediately - update DOM directly first to prevent any visual lag
		messageInputRef.current = "";
		if (selectedChatSessionId) {
			setDraft(selectedChatSessionId, "");
		}
		if (chatInputRef.current) {
			chatInputRef.current.value = "";
			chatInputRef.current.style.height = "36px";
		}
		// Then sync state (this will also trigger resize via RAF but DOM is already correct)
		setMessageInputState("");
		// Ensure any stubborn uncontrolled DOM state resets
		setChatInputMountKey((k) => k + 1);

		// Reset flag after a microtask to allow React to process any pending events
		queueMicrotask(() => {
			ignoringInputRef.current = false;
		});
		setPendingUploads([]);
		setFileAttachments([]);
		setIssueAttachments([]);
		setAgentTarget(null);
		setChatState("sending");
		setStatus("");

		// Track if we're injecting an agent response
		let effectiveMessageText = messageText;

		// If an agent target is set, ask that agent and optionally inject the response
		if (currentAgentTarget) {
			// Check if we should skip injecting the reply (Ctrl/Cmd+Enter mode)
			const noReply = agentAskNoReplyRef.current;
			agentAskNoReplyRef.current = false; // Reset for next send

			try {
				setStatus(
					locale === "de"
						? `Frage ${currentAgentTarget.name}...`
						: `Asking ${currentAgentTarget.name}...`,
				);
				// Build target string based on type
				// - main-chat: "main-chat"
				// - new-session: create session first, then ask
				// - session (OpenCode): "opencode:<id>:<workspace_path>" or "opencode:<id>"
				let targetString: string;
				if (currentAgentTarget.type === "main-chat") {
					targetString = "main-chat";
				} else if (currentAgentTarget.type === "new-session") {
					// Create a new session for the workspace path first
					if (!currentAgentTarget.workspace_path) {
						throw new Error("New session requires a workspace path");
					}
					setStatus(
						locale === "de"
							? `Erstelle Session in ${currentAgentTarget.workspace_path}...`
							: `Creating session in ${currentAgentTarget.workspace_path}...`,
					);
					const newSession = await getOrCreateSessionForWorkspace(
						currentAgentTarget.workspace_path,
					);
					targetString = `opencode:${newSession.id}:${currentAgentTarget.workspace_path}`;
					// Update status to show we're now asking
					setStatus(
						locale === "de"
							? `Frage ${currentAgentTarget.name}...`
							: `Asking ${currentAgentTarget.name}...`,
					);
				} else if (currentAgentTarget.workspace_path) {
					targetString = `opencode:${currentAgentTarget.id}:${currentAgentTarget.workspace_path}`;
				} else {
					targetString = `opencode:${currentAgentTarget.id}`;
				}
				const response = await askAgent({
					target: targetString,
					question: messageText,
					timeout_secs: 300,
				});
				setStatus("");

				if (noReply) {
					// Just show the response in a toast, don't inject into current chat
					toast.success(`Response from ${currentAgentTarget.name}`, {
						description:
							response.response.slice(0, 300) +
							(response.response.length > 300 ? "..." : ""),
						duration: 15000,
					});
					setChatState("idle");
					return;
				}

				// Format the response as a message to inject into current chat
				// This allows the current agent to see and respond to it
				effectiveMessageText = `I asked @@${currentAgentTarget.name}:\n> ${messageText}\n\nTheir response:\n${response.response}`;

				console.log(
					"[@@agent] Got response, effectiveMessageText:",
					effectiveMessageText.slice(0, 200),
				);
				// Fall through to normal send flow below with the formatted message
			} catch (err) {
				const message = err instanceof Error ? err.message : "Agent ask failed";
				setStatus(message);
				toast.error(`Failed to ask ${currentAgentTarget.name}`, {
					description: message,
				});
				setChatState("idle");
				return;
			}
		}

		try {
			let effectiveBaseUrl = opencodeBaseUrl;
			let effectiveDirectory = opencodeDirectory;
			let targetSessionId: string;

			// Main Chat mode: get workspace path from assistant info
			if (mainChatActive && mainChatAssistantName) {
				const assistantInfo = await getMainChatAssistant(mainChatAssistantName);
				const workspacePath = assistantInfo.path;
				effectiveDirectory = workspacePath;
				setMainChatWorkspacePath(workspacePath);

				setStatus(
					locale === "de" ? "Starte Main Chat..." : "Starting Main Chat...",
				);
				const url = await ensureOpencodeRunning();
				if (!url) {
					throw new Error("Failed to start Main Chat session");
				}
				effectiveBaseUrl = url;
				setMainChatBaseUrl(url);

				// If no current session, create one with a title prefix
				let resolvedMainChatSessionId = mainChatCurrentSessionId;
				if (resolvedMainChatSessionId) {
					const sessions = await fetchSessions(effectiveBaseUrl, {
						directory: effectiveDirectory,
					});
					const mainSessions = sessions.filter(
						(session) => session.directory === effectiveDirectory,
					);
					const matched = mainSessions.find(
						(session) => session.id === resolvedMainChatSessionId,
					);
					const readableMatch = mainSessions.find(
						(session) =>
							resolveReadableId(session.id, session.readable_id) ===
							resolvedMainChatSessionId,
					);
					const resolved = matched ?? readableMatch;
					if (resolved) {
						if (resolved.id !== resolvedMainChatSessionId) {
							setMainChatCurrentSessionId(resolved.id);
						}
						resolvedMainChatSessionId = resolved.id;
					} else {
						resolvedMainChatSessionId = null;
						setMainChatCurrentSessionId(null);
					}
				}

				if (!resolvedMainChatSessionId) {
					const sessionTitle = `[${mainChatAssistantName}] ${new Date().toLocaleDateString()}`;
					const newSession = await createSession(
						effectiveBaseUrl,
						sessionTitle,
						undefined,
						{ directory: effectiveDirectory },
					);

					// Register with Main Chat backend
					await registerMainChatSession(mainChatAssistantName, {
						session_id: newSession.id,
						title: sessionTitle,
					});

					// Update the current session ID
					setMainChatCurrentSessionId(newSession.id);
					targetSessionId = newSession.id;
				} else {
					targetSessionId = resolvedMainChatSessionId;
				}
				setStatus("");
			} else if (isHistoryOnlySession) {
				// Regular session: history-only, need to resume
				const workspacePath = selectedChatFromHistory?.workspace_path;
				if (!workspacePath) {
					throw new Error("Cannot resume session: no workspace path found");
				}
				effectiveDirectory = workspacePath;

				setStatus(
					locale === "de"
						? "Session wird wiederhergestellt..."
						: "Resuming session...",
				);
				const url = await ensureOpencodeRunning(workspacePath);
				if (!url) {
					throw new Error("Failed to resume workspace session");
				}
				effectiveBaseUrl = url;
				targetSessionId = selectedChatSessionId;
				setStatus("");
			} else if (!effectiveBaseUrl) {
				// Get workspace path from history session
				const workspacePath = selectedChatFromHistory?.workspace_path;
				if (!workspacePath) {
					throw new Error("Cannot resume session: no workspace path found");
				}
				effectiveDirectory = workspacePath;

				// Start opencode for this workspace
				setStatus(
					locale === "de" ? "Starte OpenCode..." : "Starting OpenCode...",
				);
				const url = await ensureOpencodeRunning(workspacePath);
				if (!url) {
					throw new Error("Failed to start OpenCode for this workspace");
				}
				effectiveBaseUrl = url;
				targetSessionId = selectedChatSessionId;
				setStatus("");
			} else {
				targetSessionId = selectedChatSessionId;
			}

			lastActiveChatSessionRef.current = targetSessionId;
			if (effectiveBaseUrl) {
				lastActiveOpencodeBaseUrlRef.current = effectiveBaseUrl;
			}

			// Touch activity to prevent idle timeout while user is active
			if (selectedWorkspaceSessionId) {
				touchSessionActivity(selectedWorkspaceSessionId).catch(() => {});
			}

			// Optimistic update - show user message immediately (now that we have the session ID)
			const optimisticMessage: OpenCodeMessageWithParts = {
				info: {
					id: `temp-${Date.now()}`,
					sessionID: targetSessionId,
					role: "user",
					time: { created: Date.now() },
				},
				parts: [
					{
						id: `temp-part-${Date.now()}`,
						sessionID: targetSessionId,
						messageID: `temp-${Date.now()}`,
						type: "text",
						text: effectiveMessageText,
					},
				],
			};

			setMessages((prev) => [...prev, optimisticMessage]);

			// Clear draft cache for this session since message was sent
			if (targetSessionId) {
				setDraft(targetSessionId, "");
			}

			// Scroll to bottom immediately
			setTimeout(() => scrollToBottom(), 50);

			if (isShellCommand && shellCommand) {
				// Run shell command via opencode shell endpoint using "build" agent
				const agentId = defaultAgent || "build";
				console.log(
					"Running shell command with agent:",
					agentId,
					"command:",
					shellCommand,
				);
				await runShellCommandAsync(
					effectiveBaseUrl,
					targetSessionId,
					shellCommand,
					agentId,
					selectedModelOverride,
					{ directory: effectiveDirectory },
				);
			} else if (currentFileAttachments.length > 0) {
				// Send with file parts
				const parts: OpenCodePartInput[] = [
					{ type: "text", text: effectiveMessageText },
				];
				// Add file parts
				for (const attachment of currentFileAttachments) {
					parts.push({
						type: "file",
						mime: "text/plain", // Will be determined by backend
						url: `file://${effectiveDirectory}/${attachment.path}`,
						filename: attachment.filename,
					});
				}
				await sendPartsAsync(
					effectiveBaseUrl,
					targetSessionId,
					parts,
					selectedModelOverride,
					{ directory: effectiveDirectory },
				);
			} else {
				// Use async send - the response will come via SSE events
				console.log(
					"[@@agent] Sending to session:",
					targetSessionId,
					"message:",
					effectiveMessageText.slice(0, 100),
				);
				await sendMessageAsync(
					effectiveBaseUrl,
					targetSessionId,
					effectiveMessageText,
					selectedModelOverride,
					{ directory: effectiveDirectory },
				);
			}
			// Invalidate cache and refresh messages to get the real message IDs
			invalidateMessageCache(
				effectiveBaseUrl,
				targetSessionId,
				effectiveDirectory,
			);

			// Fetch messages directly using the base URL we just used (not loadMessages which
			// uses stale state values that haven't updated yet after resuming a session)
			try {
				const freshMessages = await fetchMessages(
					effectiveBaseUrl,
					targetSessionId,
					{ skipCache: true, directory: effectiveDirectory },
				);
				setMessages((prev) => mergeMessages(prev, freshMessages));
			} catch {
				// Fallback to disk history if live fetch fails
				loadMessages();
			}
		} catch (err) {
			const message =
				err instanceof Error ? err.message : "Failed to send message";
			setStatus(message);
			toast.error(
				locale === "de" ? "Senden fehlgeschlagen" : "Failed to send message",
				{ description: message },
			);
			setChatState("idle");
			// Remove optimistic message on error
			setMessages((prev) => prev.filter((m) => !m.info.id.startsWith("temp-")));
		}
		// Don't set idle here - wait for SSE session.idle event
	};

	// Ref to hold latest handleSend for stable callback
	const handleSendRef = useRef(handleSend);
	handleSendRef.current = handleSend;

	// Update voiceSendRef to point to current handleSend
	voiceSendRef.current = handleSend;

	// Memoized input change handler to prevent re-renders
	const handleInputChange = useCallback(
		(e: React.ChangeEvent<HTMLTextAreaElement>) => {
			// Ignore stale onChange events that fire after send cleared the input
			if (ignoringInputRef.current) {
				// Force textarea to stay empty
				e.target.value = "";
				return;
			}

			const value = e.target.value;

			// Update ref immediately for responsive feel
			messageInputRef.current = value;

			// Keep textarea stable during dictation to avoid reflow storms.
			if (dictation.isActive) {
				e.target.style.height = "36px";
				syncInputToState(value);
			} else {
				setMessageInputWithResize(value);
			}

			// Debounce draft persistence to localStorage (300ms)
			if (draftSaveTimeoutRef.current) {
				clearTimeout(draftSaveTimeoutRef.current);
			}
			const draftToken = ++draftWriteTokenRef.current;
			draftSaveTimeoutRef.current = setTimeout(() => {
				if (draftWriteTokenRef.current !== draftToken) return;
				if (selectedChatSessionId) {
					setDraft(selectedChatSessionId, value);
				}
			}, 300);

			// Defer popup state updates to avoid blocking input
			// Only update state when values actually change to minimize re-renders
			startTransition(() => {
				// Show slash popup when typing /
				const shouldShowSlash = value.startsWith("/");
				setShowSlashPopup((prev) =>
					prev === shouldShowSlash ? prev : shouldShowSlash,
				);

				// Show agent mention popup when typing @@ (check before single @)
				const doubleAtMatch = value.match(/@@([^\s]*)$/);
				const shouldShowAgent = !!doubleAtMatch && !value.startsWith("/");
				const newAgentQuery = doubleAtMatch?.[1] ?? "";

				if (shouldShowAgent) {
					setShowAgentMentionPopup(true);
					setAgentMentionQuery((prev) =>
						prev === newAgentQuery ? prev : newAgentQuery,
					);
					setShowFileMentionPopup(false);
					setFileMentionQuery("");
				} else {
					setShowAgentMentionPopup((prev) => (prev === false ? prev : false));
					setAgentMentionQuery((prev) => (prev === "" ? prev : ""));
					// Show file mention popup when typing single @ (but not @@)
					const atMatch = value.match(/(?<!@)@([^\s@]*)$/);
					const shouldShowFile = !!atMatch && !value.startsWith("/");
					const newFileQuery = atMatch?.[1] ?? "";
					if (shouldShowFile) {
						setShowFileMentionPopup(true);
						setFileMentionQuery((prev) =>
							prev === newFileQuery ? prev : newFileQuery,
						);
					} else {
						setShowFileMentionPopup((prev) => (prev === false ? prev : false));
						setFileMentionQuery((prev) => (prev === "" ? prev : ""));
					}
				}
			});
		},
		[
			selectedChatSessionId,
			setDraft,
			setMessageInputWithResize,
			dictation.isActive,
			syncInputToState,
		],
	);

	// Memoized key down handler to prevent re-renders
	const handleInputKeyDown = useCallback(
		(e: React.KeyboardEvent<HTMLTextAreaElement>) => {
			// Let slash popup handle arrow keys, enter, tab when open
			if (showSlashPopup && slashQuery.isSlash && !slashQuery.args) {
				if (
					["ArrowDown", "ArrowUp", "Enter", "Tab", "Escape"].includes(e.key)
				) {
					// Popup will handle these via its own event listener
					return;
				}
			}
			// Let file mention popup handle its keys
			if (showFileMentionPopup) {
				if (
					["ArrowDown", "ArrowUp", "Enter", "Tab", "Escape"].includes(e.key)
				) {
					// Popup handles via its own event listener
					return;
				}
			}
			// Let agent mention popup handle its keys
			if (showAgentMentionPopup) {
				if (
					["ArrowDown", "ArrowUp", "Enter", "Tab", "Escape"].includes(e.key)
				) {
					// Popup handles via its own event listener
					return;
				}
			}
			if (e.key === "Enter" && !e.shiftKey) {
				e.preventDefault();
				// Ctrl/Cmd+Enter with agent target = send without reply
				if ((e.ctrlKey || e.metaKey) && agentTarget) {
					agentAskNoReplyRef.current = true;
				}
				handleSendRef.current();
			}
			if (e.key === "Escape") {
				setShowSlashPopup(false);
				setShowFileMentionPopup(false);
				setShowAgentMentionPopup(false);
			}
		},
		[
			showSlashPopup,
			slashQuery.isSlash,
			slashQuery.args,
			showFileMentionPopup,
			showAgentMentionPopup,
			agentTarget,
		],
	);

	const handleResume = async () => {
		if (!selectedChatSessionId || !resumeWorkspacePath) return;

		setStatus(
			locale === "de"
				? "Session wird wiederhergestellt..."
				: "Resuming session...",
		);

		try {
			const url = await ensureOpencodeRunning(resumeWorkspacePath);
			if (!url) {
				throw new Error(
					locale === "de"
						? "Sitzung konnte nicht wiederhergestellt werden"
						: "Failed to resume session",
				);
			}

			// Only fetch from opencode if we have a valid opencode session ID (starts with "ses_")
			if (selectedChatSessionId.startsWith("ses_")) {
				try {
					const liveMessages = await fetchMessages(url, selectedChatSessionId, {
						directory: resumeWorkspacePath,
					});
					if (liveMessages.length > 0) {
						setMessages((prev) => mergeMessages(prev, liveMessages));
					} else {
						await loadMessages();
					}
				} catch {
					await loadMessages();
				}
			} else {
				// Non-opencode session (Main Chat, etc.) - load from history
				await loadMessages();
			}

			setStatus("");
		} catch (err) {
			setStatus((err as Error).message);
		}
	};

	const handleSendOrResume = () => {
		const hasInput =
			messageInputRef.current.trim().length > 0 ||
			pendingUploads.length > 0 ||
			fileAttachments.length > 0 ||
			issueAttachments.length > 0;
		if (hasInput) {
			handleSend();
			return;
		}
		if (canResumeWithoutMessage) {
			handleResume();
		}
	};

	const handleStop = async () => {
		if (chatState !== "sending") return;
		const sessionId =
			lastActiveChatSessionRef.current ??
			(mainChatActive ? mainChatCurrentSessionId : selectedChatSessionId);
		const baseUrl =
			lastActiveOpencodeBaseUrlRef.current ||
			effectiveOpencodeBaseUrl ||
			opencodeBaseUrl;
		if (!baseUrl || !sessionId) return;

		try {
			await abortSession(baseUrl, sessionId, {
				directory: opencodeDirectory,
			});
			// The SSE event will set the state to idle
			// But set it immediately for responsiveness
			setChatState("idle");
			setStatus(locale === "de" ? "Abgebrochen" : "Stopped");
			// Clear status after a moment
			setTimeout(() => setStatus(""), 2000);
		} catch (err) {
			setStatus((err as Error).message);
		}
	};

	// Loading skeleton for chat view
	const ChatSkeleton = (
		<div className="flex-1 flex flex-col gap-4 min-h-0 animate-pulse">
			<div className="flex-1 bg-muted/20 p-4 space-y-6">
				{/* Skeleton message bubbles */}
				<div className="mr-8 space-y-2">
					<div className="h-4 bg-muted/40 w-24" />
					<div className="h-16 bg-muted/30" />
				</div>
				<div className="ml-8 space-y-2">
					<div className="h-4 bg-muted/40 w-16 ml-auto" />
					<div className="h-10 bg-muted/30" />
				</div>
				<div className="mr-8 space-y-2">
					<div className="h-4 bg-muted/40 w-24" />
					<div className="h-24 bg-muted/30" />
				</div>
			</div>
			<div className="h-10 bg-muted/20" />
		</div>
	);

	// Loading skeleton for sidebar
	const SidebarSkeleton = (
		<div className="flex-1 flex flex-col animate-pulse">
			<div className="flex gap-1 p-2">
				{[1, 2, 3, 4].map((i) => (
					<div key={i} className="flex-1 h-8 bg-muted/30" />
				))}
			</div>
			<div className="flex-1 p-4 space-y-3">
				<div className="h-4 bg-muted/40 w-3/4" />
				<div className="h-4 bg-muted/40 w-1/2" />
				<div className="h-4 bg-muted/40 w-2/3" />
				<div className="h-32 bg-muted/30 mt-4" />
			</div>
		</div>
	);

	const showExpandedPreview =
		expandedView === "preview" && Boolean(previewFilePath);
	const showExpandedCanvas = expandedView === "canvas";
	const showExpandedMemories =
		expandedView === "memories" && features.mmry_enabled;
	const showExpandedTerminal = expandedView === "terminal";
	const chatInSidebar = !isMobileLayout && expandedView !== null;

	const expandedPanel = (
		<div className="flex flex-col h-full overflow-hidden">
			{showExpandedPreview && (
				<Suspense fallback={viewLoadingFallback}>
					<PreviewView
						filePath={previewFilePath}
						workspacePath={resumeWorkspacePath}
						onClose={closePreview}
						onToggleExpand={() => toggleExpandedView("preview")}
						isExpanded
						showExpand={!isMobileLayout}
					/>
				</Suspense>
			)}
			{showExpandedCanvas && (
				<div className="flex flex-col h-full overflow-hidden">
					{!isMobileLayout && (
						<div className="flex items-center justify-between px-2 py-1 pr-10 border-b border-border bg-muted/30">
							<span className="text-xs text-muted-foreground">Canvas</span>
							<button
								type="button"
								onClick={() => toggleExpandedView("canvas")}
								className="p-1 text-muted-foreground hover:text-foreground hover:bg-muted/50"
								aria-label="Collapse canvas"
							>
								<Minimize2 className="w-3.5 h-3.5" />
							</button>
						</div>
					)}
					<div className="flex-1 min-h-0">
						<Suspense fallback={viewLoadingFallback}>
							<CanvasView
								workspacePath={resumeWorkspacePath}
								initialImagePath={previewFilePath}
								onSaveAndAddToChat={handleCanvasSaveAndAddToChat}
							/>
						</Suspense>
					</div>
				</div>
			)}
			{showExpandedMemories && (
				<div className="flex flex-col h-full overflow-hidden">
					{!isMobileLayout && (
						<div className="flex items-center justify-between px-2 py-1 pr-10 border-b border-border bg-muted/30">
							<span className="text-xs text-muted-foreground">
								{t.memories}
							</span>
							<button
								type="button"
								onClick={() => toggleExpandedView("memories")}
								className="p-1 text-muted-foreground hover:text-foreground hover:bg-muted/50"
								aria-label="Collapse memories"
							>
								<Minimize2 className="w-3.5 h-3.5" />
							</button>
						</div>
					)}
					<div className="flex-1 min-h-0">
						<Suspense fallback={viewLoadingFallback}>
							<MemoriesView
								workspacePath={resumeWorkspacePath}
								storeName={null}
							/>
						</Suspense>
					</div>
				</div>
			)}
			{showExpandedTerminal && (
				<div className="flex flex-col h-full overflow-hidden">
					{!isMobileLayout && (
						<div className="flex items-center justify-between px-2 py-1 pr-10 border-b border-border bg-muted/30">
							<span className="text-xs text-muted-foreground">
								{t.terminal}
							</span>
							<button
								type="button"
								onClick={() => toggleExpandedView("terminal")}
								className="p-1 text-muted-foreground hover:text-foreground hover:bg-muted/50"
								aria-label="Collapse terminal"
							>
								<Minimize2 className="w-3.5 h-3.5" />
							</button>
						</div>
					)}
					<div className="flex-1 min-h-0">
						<Suspense fallback={viewLoadingFallback}>
							<TerminalView workspacePath={resumeWorkspacePath} />
						</Suspense>
					</div>
				</div>
			)}
		</div>
	);

	useEffect(() => {
		if (chatInSidebar && rightSidebarCollapsed) {
			setRightSidebarCollapsed(false);
		}
	}, [chatInSidebar, rightSidebarCollapsed]);

	// Show loading skeleton or error only if we have no sessions AND no chat history
	if (workspaceSessions.length === 0 && chatHistory.length === 0) {
		// Show skeleton while loading, project selector after timeout
		return (
			<div className="flex flex-col h-full min-h-0 p-1 sm:p-4 md:p-6 gap-1 sm:gap-4">
				{showTimeoutError ? (
					<div className="p-4 md:p-6 max-w-2xl mx-auto w-full">
						<div className="p-6 bg-card border border-border rounded-lg">
							<h2 className="text-lg font-medium mb-4">
								{locale === "de" ? "Projekt auswählen" : "Select a Project"}
							</h2>
							{projects.length > 0 ? (
								<div className="grid gap-2 max-h-[60vh] overflow-y-auto">
									{projects.map((project) => {
										const logoUrl = project.logo
											? getProjectLogoUrl(project.path, project.logo.path)
											: null;
										return (
											<button
												type="button"
												key={project.path}
												onClick={() => startProjectSession(project.path)}
												className="flex items-center gap-3 p-3 text-left rounded-md border border-border hover:bg-muted/50 transition-colors"
											>
												<div className="h-8 w-8 flex-shrink-0 flex items-center justify-center rounded bg-muted/50 overflow-hidden">
													{logoUrl ? (
														<img
															src={logoUrl}
															alt={`${project.name} logo`}
															className="h-6 w-6 object-contain"
														/>
													) : (
														<FileText className="h-5 w-5 text-muted-foreground" />
													)}
												</div>
												<div className="min-w-0">
													<div className="font-medium truncate">
														{project.name}
													</div>
													<div className="text-sm text-muted-foreground truncate">
														{project.path}
													</div>
												</div>
											</button>
										);
									})}
								</div>
							) : (
								<div className="space-y-4">
									<div className="text-sm text-muted-foreground">
										{t.configNotice}
									</div>
									{/* Connection diagnostics for debugging (especially iOS) */}
									<div className="mt-4 p-3 bg-muted/30 rounded text-xs font-mono space-y-1">
										<div className="font-semibold text-foreground mb-2">
											Connection Diagnostics:
										</div>
										<div>
											<span className="text-muted-foreground">
												Backend URL:
											</span>{" "}
											<span className="text-foreground break-all">
												{connectionDiagnostics.controlPlaneUrl || "(not set)"}
											</span>
										</div>
										<div>
											<span className="text-muted-foreground">
												Features loaded:
											</span>{" "}
											<span
												className={
													connectionDiagnostics.featuresLoaded
														? "text-green-500"
														: "text-red-500"
												}
											>
												{connectionDiagnostics.featuresLoaded ? "Yes" : "No"}
											</span>
										</div>
										{connectionDiagnostics.lastError && (
											<div>
												<span className="text-muted-foreground">
													Last error:
												</span>{" "}
												<span className="text-red-500 break-all">
													{connectionDiagnostics.lastError}
												</span>
											</div>
										)}
										<div>
											<span className="text-muted-foreground">Sessions:</span>{" "}
											<span className="text-foreground">
												{workspaceSessions.length} workspace,{" "}
												{chatHistory.length} chat history
											</span>
										</div>
										<div>
											<span className="text-muted-foreground">Projects:</span>{" "}
											<span className="text-foreground">{projects.length}</span>
										</div>
									</div>
								</div>
							)}
						</div>
					</div>
				) : (
					<>
						{/* Mobile skeleton */}
						<div className="flex-1 min-h-0 flex flex-col lg:hidden">
							<div className="sticky top-0 z-10 flex gap-0.5 p-1 sm:p-2 bg-muted/10">
								{[1, 2, 3, 4, 5].map((i) => (
									<div
										key={i}
										className="flex-1 h-7 bg-muted/30 animate-pulse"
									/>
								))}
							</div>
							<div className="flex-1 min-h-0 bg-muted/10 p-1.5 sm:p-4 overflow-hidden">
								{ChatSkeleton}
							</div>
						</div>

						{/* Desktop skeleton */}
						<div className="hidden lg:flex flex-1 min-h-0 gap-4 items-start">
							<div className="flex-[3] min-w-0 bg-muted/10 p-4 xl:p-6 flex flex-col min-h-0 h-full">
								{ChatSkeleton}
							</div>
							<div className="flex-[2] min-w-[320px] max-w-[420px] bg-muted/10 flex flex-col min-h-0 h-full">
								{SidebarSkeleton}
							</div>
						</div>
					</>
				)}
			</div>
		);
	}

	// Session metadata for chat display
	const readableIdSource =
		selectedChatSession ?? selectedChatFromHistory ?? null;
	const readableId = readableIdSource
		? resolveReadableId(readableIdSource.id, readableIdSource.readable_id)
		: null;
	// Extract workspace name from path (last segment)
	const workspaceName =
		(isWorkspacePiSession ? workspacePiPath : opencodeDirectory)
			?.split("/")
			.filter(Boolean)
			.pop() || null;

	// Chat content component (reused in both layouts)
	const renderChatContent = (allowExpanded: boolean) => {
		if (isWorkspacePiSession) {
			return (
				<MainChatPiView
					locale={locale}
					className="flex-1"
					features={features}
					workspacePath={workspacePiPath}
					hideHeader
					scope="workspace"
					storageKeyPrefix={workspacePiStorageKeyPrefix}
					selectedSessionId={selectedChatSessionId}
					onSelectedSessionIdChange={handleWorkspacePiSessionChange}
					scrollToMessageId={scrollToMessageId}
					onScrollToMessageComplete={() => setScrollToMessageId(null)}
					onTokenUsageChange={setWorkspacePiTokenUsage}
					onTodosChange={setWorkspacePiTodos}
					onMessageSent={refreshChatHistory}
				/>
			);
		}
		return (
			<div
				ref={chatContainerRef}
				className="flex-1 flex flex-col gap-2 sm:gap-4 min-h-0"
			>
				{/* Permission banner */}
				<PermissionBanner
					count={pendingPermissions.length}
					onClick={handlePermissionBannerClick}
				/>

				{/* User question banner */}
				<UserQuestionBanner
					count={pendingQuestions.length}
					onClick={handleQuestionBannerClick}
				/>

				<div className="relative flex-1 min-h-0">
					{allowExpanded && showExpandedPreview ? (
						<div className="h-full bg-muted/30 border border-border overflow-hidden">
							<Suspense fallback={viewLoadingFallback}>
								<PreviewView
									filePath={previewFilePath}
									workspacePath={resumeWorkspacePath}
									onClose={closePreview}
									onToggleExpand={() => toggleExpandedView("preview")}
									isExpanded
									showExpand={!isMobileLayout}
								/>
							</Suspense>
						</div>
					) : allowExpanded && showExpandedCanvas ? (
						<div className="h-full bg-muted/30 border border-border overflow-hidden flex flex-col">
							{!isMobileLayout && (
								<div className="flex items-center justify-between px-2 py-1 border-b border-border bg-muted/30">
									<span className="text-xs text-muted-foreground">Canvas</span>
									<button
										type="button"
										onClick={() => toggleExpandedView("canvas")}
										className="p-1 text-muted-foreground hover:text-foreground hover:bg-muted/50"
										aria-label="Collapse canvas"
									>
										<Minimize2 className="w-3.5 h-3.5" />
									</button>
								</div>
							)}
							<div className="flex-1 min-h-0">
								<Suspense fallback={viewLoadingFallback}>
									<CanvasView
										workspacePath={resumeWorkspacePath}
										initialImagePath={previewFilePath}
										onSaveAndAddToChat={handleCanvasSaveAndAddToChat}
									/>
								</Suspense>
							</div>
						</div>
					) : allowExpanded && showExpandedMemories ? (
						<div className="h-full bg-muted/30 border border-border overflow-hidden flex flex-col">
							{!isMobileLayout && (
								<div className="flex items-center justify-between px-2 py-1 border-b border-border bg-muted/30">
									<span className="text-xs text-muted-foreground">
										{t.memories}
									</span>
									<button
										type="button"
										onClick={() => toggleExpandedView("memories")}
										className="p-1 text-muted-foreground hover:text-foreground hover:bg-muted/50"
										aria-label="Collapse memories"
									>
										<Minimize2 className="w-3.5 h-3.5" />
									</button>
								</div>
							)}
							<div className="flex-1 min-h-0">
								<Suspense fallback={viewLoadingFallback}>
									<MemoriesView
										workspacePath={resumeWorkspacePath}
										storeName={null}
									/>
								</Suspense>
							</div>
						</div>
					) : allowExpanded && showExpandedTerminal ? (
						<div className="h-full bg-muted/30 border border-border overflow-hidden flex flex-col">
							{!isMobileLayout && (
								<div className="flex items-center justify-between px-2 py-1 border-b border-border bg-muted/30">
									<span className="text-xs text-muted-foreground">
										{t.terminal}
									</span>
									<button
										type="button"
										onClick={() => toggleExpandedView("terminal")}
										className="p-1 text-muted-foreground hover:text-foreground hover:bg-muted/50"
										aria-label="Collapse terminal"
									>
										<Minimize2 className="w-3.5 h-3.5" />
									</button>
								</div>
							)}
							<div className="flex-1 min-h-0">
								<Suspense fallback={viewLoadingFallback}>
									<TerminalView workspacePath={resumeWorkspacePath} />
								</Suspense>
							</div>
						</div>
					) : (
						<ChatMessagesPane
							messages={messages}
							messagesLoading={messagesLoading}
							selectedChatSessionId={selectedChatSessionId ?? undefined}
							sessionHadMessages={Boolean(
								selectedChatSessionId &&
									sessionsWithMessagesRef.current.has(selectedChatSessionId),
							)}
							hasHiddenMessages={hasHiddenMessages}
							messageGroupsLength={messageGroups.length}
							visibleGroups={visibleGroups}
							visibleGroupCount={visibleGroupCount}
							a2uiByGroupIndex={a2uiByGroupIndex}
							locale={locale}
							noMessagesText={t.noMessages}
							persona={selectedSession?.persona}
							workspaceName={workspaceName}
							readableId={readableId}
							workspaceDirectory={opencodeDirectory}
							onFork={handleForkSession}
							onScroll={handleScroll}
							messagesContainerRef={messagesContainerRef}
							messagesEndRef={messagesEndRef}
							showScrollToBottom={showScrollToBottom}
							scrollToBottom={scrollToBottom}
							loadMoreMessages={loadMoreMessages}
							onA2UIAction={handleA2UIAction}
							isStreaming={chatState === "sending"}
						/>
					)}
				</div>

				{/* Pending uploads indicator */}
				{pendingUploads.length > 0 && (
					<div className="flex flex-wrap gap-2 mb-2">
						{pendingUploads.map((upload) => (
							<div
								key={upload.path}
								className="flex items-center gap-1.5 px-2 py-1 bg-primary/10 border border-primary/30 text-xs text-foreground"
							>
								<Paperclip className="w-3 h-3 text-primary" />
								<span className="truncate max-w-[150px]">{upload.name}</span>
								<button
									type="button"
									onClick={() => removePendingUpload(upload.path)}
									className="text-muted-foreground hover:text-foreground ml-1"
								>
									<X className="w-3 h-3" />
								</button>
							</div>
						))}
					</div>
				)}

				{/* Hidden file input */}
				<input
					ref={fileInputRef}
					type="file"
					multiple
					className="hidden"
					onChange={(e) => handleFileUpload(e.target.files)}
				/>

				{/* Chat input - works for both live and history sessions */}
				<div className="chat-input-container flex flex-col gap-1 bg-muted/30 border border-border px-2 py-1">
					{/* Show hint for history sessions that will be resumed - hide when sending/resuming */}
					{isHistoryOnlySession && chatState === "idle" && (
						<div className="flex items-center gap-1.5 px-1 pt-1 text-xs text-muted-foreground">
							<Clock className="w-3 h-3" />
							<span>
								{locale === "de"
									? canResumeWithoutMessage
										? "Nachricht senden oder ohne Nachricht fortsetzen"
										: "Sende eine Nachricht um diese Sitzung fortzusetzen"
									: canResumeWithoutMessage
										? "Send a message or resume without one"
										: "Send a message to resume this session"}
							</span>
						</div>
					)}
					<div className="flex items-center gap-2">
						<button
							type="button"
							onClick={() => fileInputRef.current?.click()}
							disabled={isUploading}
							className="flex-shrink-0 h-8 px-2 flex items-center justify-center text-muted-foreground hover:text-foreground disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
							title={locale === "de" ? "Datei hochladen" : "Upload file"}
						>
							{isUploading ? (
								<Loader2 className="size-4 animate-spin" />
							) : (
								<Paperclip className="size-4" />
							)}
						</button>
						{/* Unified voice menu button - conversation or dictation */}
						{features.voice && (
							<VoiceMenuButton
								activeMode={
									voiceMode.isActive
										? "conversation"
										: dictation.isActive
											? "dictation"
											: null
								}
								voiceState={voiceMode.voiceState}
								onConversation={() => {
									if (dictation.isActive) dictation.stop();
									voiceMode.start().catch(console.error);
								}}
								onDictation={() => {
									if (voiceMode.isActive) voiceMode.stop();
									dictation.start().catch(console.error);
								}}
								onStop={() => {
									if (voiceMode.isActive) voiceMode.stop();
									if (dictation.isActive) dictation.stop();
								}}
								locale={locale}
								className="flex-shrink-0"
							/>
						)}
						{/* Textarea wrapper with slash command popup */}
						<div
							className="flex-1 relative flex flex-col min-h-[32px]"
							data-spotlight="chat-input"
						>
							<SlashCommandPopup
								commands={slashCommands}
								query={slashQuery.command}
								isOpen={
									showSlashPopup && slashQuery.isSlash && !slashQuery.args
								}
								onSelect={handleSlashCommandSelect}
								onClose={() => setShowSlashPopup(false)}
							/>
							<FileMentionPopup
								query={fileMentionQuery}
								isOpen={showFileMentionPopup}
								workspacePath={resumeWorkspacePath}
								onSelect={(attachment) => {
									// Remove @query from input, only show chip
									const newInput = messageInput.replace(/@[^\s]*$/, "");
									setMessageInputWithResize(newInput);
									setFileAttachments((prev) => [...prev, attachment]);
									setShowFileMentionPopup(false);
									setFileMentionQuery("");
									chatInputRef.current?.focus();
								}}
								onClose={() => {
									setShowFileMentionPopup(false);
									setFileMentionQuery("");
								}}
							/>
							<AgentMentionPopup
								query={agentMentionQuery}
								isOpen={showAgentMentionPopup}
								mainChatName={mainChatAssistantName}
								mainChatWorkspacePath={mainChatWorkspacePath}
								sessions={chatHistory.map((s) => ({
									id: s.id,
									title: s.title,
									workspace_path: s.workspace_path,
									project_name: s.project_name,
								}))}
								onSelect={(target) => {
									// Remove @@query from input, store target
									// Use ref value directly since debounced state may be stale
									const newInput = messageInputRef.current.replace(
										/@@[^\s]*$/,
										"",
									);
									setMessageInputWithResize(newInput);
									messageInputRef.current = newInput;
									setAgentTarget(target);
									setShowAgentMentionPopup(false);
									setAgentMentionQuery("");
									chatInputRef.current?.focus();
								}}
								onClose={() => {
									setShowAgentMentionPopup(false);
									setAgentMentionQuery("");
								}}
							/>
							{/* File attachment chips */}
							{fileAttachments.length > 0 && (
								<div className="flex flex-wrap gap-1 mb-1">
									{fileAttachments.map((attachment) => (
										<FileAttachmentChip
											key={attachment.id}
											attachment={attachment}
											onRemove={() => {
												setFileAttachments((prev) =>
													prev.filter((a) => a.id !== attachment.id),
												);
											}}
										/>
									))}
								</div>
							)}
							{/* Issue attachment chips */}
							{issueAttachments.length > 0 && (
								<div className="flex flex-wrap gap-1 mb-1">
									{issueAttachments.map((attachment) => (
										<IssueAttachmentChip
											key={attachment.id}
											attachment={attachment}
											onRemove={() => {
												setIssueAttachments((prev) =>
													prev.filter((a) => a.id !== attachment.id),
												);
											}}
										/>
									))}
								</div>
							)}
							{/* Agent target chip (@@mention) */}
							{agentTarget && (
								<div className="flex flex-wrap gap-1 mb-1">
									<AgentTargetChip
										target={agentTarget}
										onRemove={() => setAgentTarget(null)}
									/>
								</div>
							)}
							{features.voice && dictation.isActive ? (
								<DictationOverlay
									open
									value={messageInputRef.current}
									liveTranscript={dictation.liveTranscript}
									placeholder={
										locale === "de" ? "Sprechen Sie..." : "Speak now..."
									}
									vadProgress={dictation.vadProgress}
									autoSend={dictation.autoSendEnabled}
									onAutoSendChange={dictation.setAutoSendEnabled}
									onStop={() => {
										// Use cancel() to stop without auto-send - user clicked X
										dictation.cancel();
										requestAnimationFrame(() => {
											setMessageInputWithResize(messageInputRef.current);
										});
									}}
									onChange={handleInputChange}
									onKeyDown={handleInputKeyDown}
									onPaste={(e) => {
										// Handle pasted files (images, etc.)
										const items = e.clipboardData?.items;
										if (!items) return;

										const files: File[] = [];
										let imageIndex = 0;
										for (const item of Array.from(items)) {
											if (item.kind === "file") {
												const file = item.getAsFile();
												if (file) {
													// Rename generic clipboard image names to be unique
													const isGenericName =
														/^image\.(png|gif|jpg|jpeg|webp)$/i.test(file.name);
													if (isGenericName) {
														const ext = file.name.split(".").pop() || "png";
														const uniqueName = `pasted-image-${Date.now()}-${imageIndex++}.${ext}`;
														const renamedFile = new File([file], uniqueName, {
															type: file.type,
														});
														files.push(renamedFile);
													} else {
														files.push(file);
													}
												}
											}
										}

										if (files.length > 0) {
											// Prevent default paste behavior for files
											e.preventDefault();
											// Create a FileList-like object and upload
											const dataTransfer = new DataTransfer();
											for (const file of files) {
												dataTransfer.items.add(file);
											}
											handleFileUpload(dataTransfer.files);
										}
										// If no files, let the default paste behavior handle text
									}}
									onBlur={() => {
										// Delay closing to allow click on popup items
										setTimeout(() => setShowSlashPopup(false), 150);
									}}
									onFocus={(e) => {
										// Scroll input into view on mobile when keyboard opens
										setTimeout(() => {
											e.target.scrollIntoView({
												behavior: "smooth",
												block: "nearest",
											});
										}, 300);
									}}
								/>
							) : (
								<textarea
									key={`${chatInputMountKey}-${selectedChatSessionId || "none"}`}
									ref={setChatInputEl}
									autoComplete="off"
									autoCorrect="off"
									autoCapitalize="sentences"
									spellCheck={false}
									enterKeyHint="send"
									data-form-type="other"
									placeholder={
										isHistoryOnlySession
											? locale === "de"
												? "Nachricht zum Fortsetzen..."
												: "Message to resume..."
											: t.inputPlaceholder
									}
									defaultValue=""
									onChange={handleInputChange}
									onKeyDown={handleInputKeyDown}
									onPaste={(e) => {
										// Handle pasted files (images, etc.)
										const items = e.clipboardData?.items;
										if (!items) return;

										const files: File[] = [];
										let imageIndex = 0;
										for (const item of Array.from(items)) {
											if (item.kind === "file") {
												const file = item.getAsFile();
												if (file) {
													// Rename generic clipboard image names to be unique
													const isGenericName =
														/^image\.(png|gif|jpg|jpeg|webp)$/i.test(file.name);
													if (isGenericName) {
														const ext = file.name.split(".").pop() || "png";
														const uniqueName = `pasted-image-${Date.now()}-${imageIndex++}.${ext}`;
														const renamedFile = new File([file], uniqueName, {
															type: file.type,
														});
														files.push(renamedFile);
													} else {
														files.push(file);
													}
												}
											}
										}

										if (files.length > 0) {
											// Prevent default paste behavior for files
											e.preventDefault();
											// Create a FileList-like object and upload
											const dataTransfer = new DataTransfer();
											for (const file of files) {
												dataTransfer.items.add(file);
											}
											handleFileUpload(dataTransfer.files);
										}
										// If no files, let the default paste behavior handle text
									}}
									onBlur={() => {
										// Delay closing to allow click on popup items
										setTimeout(() => setShowSlashPopup(false), 150);
									}}
									onFocus={(e) => {
										// Show popup if input starts with /
										if (deferredMessageInput.startsWith("/")) {
											setShowSlashPopup(true);
										}
										// Scroll input into view on mobile when keyboard opens
										setTimeout(() => {
											e.target.scrollIntoView({
												behavior: "smooth",
												block: "nearest",
											});
										}, 300);
									}}
									rows={1}
									className="w-full bg-transparent border-none outline-none text-foreground placeholder:text-muted-foreground text-sm resize-none py-1.5 leading-5 max-h-[200px] overflow-y-auto"
								/>
							)}
						</div>
						{chatState === "sending" && (
							<Button
								type="button"
								onClick={handleStop}
								className="stop-button-animated flex-shrink-0 h-8 px-2 flex items-center justify-center text-destructive hover:text-destructive/80 transition-colors bg-transparent hover:bg-transparent"
								variant="ghost"
								size="icon"
								title={
									locale === "de"
										? "Agent stoppen (2x Esc)"
										: "Stop agent (2x Esc)"
								}
							>
								<span className="stop-button-ring" aria-hidden>
									<svg viewBox="0 0 100 100" role="presentation">
										<circle
											cx="50"
											cy="50"
											r="46"
											fill="none"
											stroke="currentColor"
											strokeWidth="3"
											strokeLinecap="round"
											strokeDasharray="72 216"
											opacity="0.8"
										/>
									</svg>
								</span>
								<StopCircle className="w-4 h-4" />
							</Button>
						)}
						<Button
							type="button"
							data-voice-send
							onClick={handleSendOrResume}
							disabled={
								!canResumeWithoutMessage &&
								!deferredMessageInput.trim() &&
								pendingUploads.length === 0 &&
								fileAttachments.length === 0 &&
								issueAttachments.length === 0
							}
							className="flex-shrink-0 h-8 px-2 flex items-center justify-center text-muted-foreground hover:text-foreground disabled:opacity-50 disabled:cursor-not-allowed transition-colors bg-transparent hover:bg-transparent"
							variant="ghost"
							size="icon"
						>
							{canResumeWithoutMessage ? (
								<RefreshCw className="w-4 h-4" />
							) : (
								<Send className="w-4 h-4" />
							)}
						</Button>
					</div>
				</div>
			</div>
		);
	};

	// Voice input overlay - shown on mobile when voice mode is active
	const mobileVoiceOverlay =
		voiceMode.isActive && features.voice && isMobileLayout ? (
			<VoiceInputOverlay
				voiceState={voiceMode.voiceState}
				liveTranscript={voiceMode.liveTranscript}
				vadProgress={voiceMode.vadProgress}
				inputVolume={voiceMode.inputVolume}
				outputVolume={voiceMode.outputVolume}
				settings={voiceMode.settings}
				availableVoices={voiceMode.availableVoices}
				onClose={voiceMode.stop}
				onInterrupt={voiceMode.interrupt}
				onSettingsChange={{
					setVisualizer: voiceMode.setVisualizer,
					setMuted: voiceMode.setMuted,
					setMicMuted: voiceMode.setMicMuted,
					setContinuous: voiceMode.setContinuous,
					setVoice: voiceMode.setVoice,
					setSpeed: voiceMode.setSpeed,
					setVadTimeout: voiceMode.setVadTimeout,
					setInterruptWordCount: voiceMode.setInterruptWordCount,
				}}
			/>
		) : null;

	// Voice panel props for desktop sidebar
	const voicePanelProps = {
		voiceState: voiceMode.voiceState,
		liveTranscript: voiceMode.liveTranscript,
		vadProgress: voiceMode.vadProgress,
		inputVolume: voiceMode.inputVolume,
		outputVolume: voiceMode.outputVolume,
		settings: voiceMode.settings,
		availableVoices: voiceMode.availableVoices,
		onClose: voiceMode.stop,
		onInterrupt: voiceMode.interrupt,
		onSettingsChange: {
			setVisualizer: voiceMode.setVisualizer,
			setMuted: voiceMode.setMuted,
			setMicMuted: voiceMode.setMicMuted,
			setContinuous: voiceMode.setContinuous,
			setVoice: voiceMode.setVoice,
			setSpeed: voiceMode.setSpeed,
			setVadTimeout: voiceMode.setVadTimeout,
			setInterruptWordCount: voiceMode.setInterruptWordCount,
		},
	};

	// Keep FileTreeView always mounted to preserve state across tab switches
	const filesView = (
		<div className="flex flex-col h-full overflow-hidden">
			<div
				className={cn("flex-1 min-h-0", previewFilePath && "hidden")}
				data-spotlight="file-tree"
			>
				<FileTreeView
					onPreviewFile={handlePreviewFile}
					onOpenInCanvas={handleOpenInCanvas}
					workspacePath={resumeWorkspacePath}
					isMainChat={mainChatActive}
					state={fileTreeState}
					onStateChange={handleFileTreeStateChange}
				/>
			</div>
			{previewFilePath && (
				<div className="flex-1 min-h-0">
					<Suspense fallback={viewLoadingFallback}>
						<PreviewView
							filePath={previewFilePath}
							workspacePath={resumeWorkspacePath}
							isMainChat={mainChatActive}
							onClose={closePreview}
							onToggleExpand={() => toggleExpandedView("preview")}
							isExpanded={expandedView === "preview"}
							showExpand={!isMobileLayout}
						/>
					</Suspense>
				</div>
			)}
		</div>
	);

	const incompleteTasks = latestTodos.filter(
		(t) => t.status !== "completed" && t.status !== "cancelled",
	).length;

	// Format session metadata for display
	const sessionCreatedAt = selectedChatSession?.time?.created;
	const formattedDate = sessionCreatedAt
		? formatSessionDate(sessionCreatedAt)
		: null;

	// Clean up session title - remove ISO timestamp suffix if present (e.g., "New session - 2025-12-18T07:46:58.478Z")
	const cleanSessionTitle = (() => {
		const title = selectedChatSession?.title;
		if (!title) return null;
		// Remove " - YYYY-MM-DDTHH:MM:SS.sssZ" pattern from the end
		return (
			title
				.replace(/\s*-\s*\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(\.\d+)?Z?$/, "")
				.trim() || null
		);
	})();

	// Session header component for reuse
	const showOpencodeModelSwitcher =
		!mainChatActive &&
		!isWorkspacePiSession &&
		!!effectiveOpencodeBaseUrl &&
		!!activeSessionId;
	const showPiModelSwitcher =
		isWorkspacePiSession && !!workspacePiPath && !!selectedChatSessionId;
	const filteredModelOptions = filterModelOptions(
		opencodeModelOptions,
		modelQuery,
	);
	const modelSwitcher = showPiModelSwitcher ? (
		<div data-spotlight="model-picker">
			<Select
				value={piSelectedModelRef ?? undefined}
				onValueChange={handlePiModelChange}
				onOpenChange={(open) => {
					if (open) setPiModelQuery("");
				}}
				disabled={
					piIsSwitchingModel ||
					piIsModelLoading ||
					!piIsIdle ||
					piModelOptions.length === 0
				}
			>
				<SelectTrigger className="h-7 w-[220px] text-xs">
					<SelectValue
						placeholder={
							piIsSwitchingModel
								? "Switching model..."
								: piIsModelLoading
									? "Loading models..."
									: "Model"
						}
					/>
				</SelectTrigger>
				<SelectContent>
					<div
						className="sticky top-0 z-10 bg-popover p-2 border-b border-border"
						onPointerDown={(e) => e.stopPropagation()}
						onKeyDown={(e) => e.stopPropagation()}
					>
						<Input
							value={piModelQuery}
							onChange={(e) => setPiModelQuery(e.target.value)}
							placeholder="Search models..."
							aria-label="Search models"
							className="h-8 text-xs"
						/>
					</div>
					{piModelOptions.length === 0 ? (
						<SelectItem value="__none__" disabled>
							{piIsModelLoading ? "Loading..." : "No models available"}
						</SelectItem>
					) : filteredPiModels.length === 0 ? (
						<SelectItem value="__no_results__" disabled>
							No matches
						</SelectItem>
					) : (
						filteredPiModels.map((model) => {
							const value = `${model.provider}/${model.id}`;
							const label = model.name ? `${value} · ${model.name}` : value;
							return (
								<SelectItem key={value} value={value} textValue={label}>
									<span className="flex items-center gap-2">
										<ProviderIcon
											provider={model.provider}
											className="w-4 h-4 flex-shrink-0"
										/>
										<span>{label}</span>
									</span>
								</SelectItem>
							);
						})
					)}
				</SelectContent>
			</Select>
		</div>
	) : showOpencodeModelSwitcher ? (
		<div data-spotlight="model-picker">
			<Select
				value={selectedModelRef ?? undefined}
				onValueChange={(value) => setSelectedModelRef(value)}
				onOpenChange={(open) => {
					if (open) setModelQuery("");
				}}
				disabled={isModelLoading || opencodeModelOptions.length === 0}
			>
				<SelectTrigger className="h-7 w-[220px] text-xs">
					<SelectValue
						placeholder={isModelLoading ? "Loading models..." : "Model"}
					/>
				</SelectTrigger>
				<SelectContent>
					<div
						className="sticky top-0 z-10 bg-popover p-2 border-b border-border"
						onPointerDown={(e) => e.stopPropagation()}
						onKeyDown={(e) => e.stopPropagation()}
					>
						<Input
							value={modelQuery}
							onChange={(e) => setModelQuery(e.target.value)}
							placeholder="Search models..."
							aria-label="Search models"
							className="h-8 text-xs"
						/>
					</div>
					{opencodeModelOptions.length === 0 ? (
						<SelectItem value="__none__" disabled>
							{isModelLoading
								? "Loading..."
								: "Start a session to select models"}
						</SelectItem>
					) : filteredModelOptions.length === 0 ? (
						<SelectItem value="__no_results__" disabled>
							No matches
						</SelectItem>
					) : (
						filteredModelOptions.map((option) => {
							const provider = option.value.split("/")[0];
							return (
								<SelectItem
									key={option.value}
									value={option.value}
									textValue={option.label}
								>
									<span className="flex items-center gap-2">
										<ProviderIcon
											provider={provider}
											className="w-4 h-4 flex-shrink-0"
										/>
										<span>{option.label}</span>
									</span>
								</SelectItem>
							);
						})
					)}
				</SelectContent>
			</Select>
		</div>
	) : null;
	const persona = selectedSession?.persona;

	const SessionHeader = (
		<div className="pb-3 mb-3 border-b border-border pr-10">
			<div className="flex items-center justify-between">
				<div className="flex items-center gap-3 min-w-0 flex-1">
					{/* Persona avatar/indicator */}
					{persona && (
						<div
							className="w-8 h-8 sm:w-10 sm:h-10 rounded-full flex items-center justify-center flex-shrink-0"
							style={{ backgroundColor: persona.color || "#6366f1" }}
						>
							<User className="w-4 h-4 sm:w-5 sm:h-5 text-white" />
						</div>
					)}
					<div className="min-w-0 flex-1">
						<div className="flex items-center gap-2">
							<h1 className="text-base sm:text-lg font-semibold text-foreground tracking-wider truncate">
								{cleanSessionTitle || t.title}
							</h1>
							{persona && (
								<span
									className="text-xs px-1.5 py-0.5 rounded-full text-white flex-shrink-0"
									style={{ backgroundColor: persona.color || "#6366f1" }}
								>
									{persona.name}
								</span>
							)}
						</div>
						<div className="flex items-center gap-2 text-xs text-foreground/60 dark:text-muted-foreground">
							{(workspaceName || readableId) && (
								<span className="font-mono truncate">
									{workspaceName}
									{readableId && ` [${readableId}]`}
								</span>
							)}
							{(workspaceName || readableId) && formattedDate && (
								<span className="opacity-50">|</span>
							)}
							{formattedDate && (
								<span className="flex-shrink-0">{formattedDate}</span>
							)}
						</div>
					</div>
				</div>
				{status && (
					<span className="text-xs text-destructive flex-shrink-0 ml-2 max-w-[150px] truncate">
						{status}
					</span>
				)}
			</div>
			{/* Context window gauge - full width bar at bottom of header */}
			<div className="mt-2">
				<ContextWindowGauge
					inputTokens={displayTokenUsage.inputTokens}
					outputTokens={displayTokenUsage.outputTokens}
					maxTokens={displayContextLimit}
					locale={locale}
					compact
				/>
			</div>
		</div>
	);

	const app = (
		<div className="flex flex-col h-full min-h-0 p-1 sm:p-4 md:p-6 gap-1 sm:gap-4">
			{/* Mobile layout: single panel with tabs */}
			<div className="flex-1 min-h-0 flex flex-col lg:hidden">
				{/* Mobile tabs - sticky at top */}
				<div className="sticky top-0 z-10 bg-card border border-border rounded-t-xl overflow-hidden">
					<div className="flex gap-0.5 p-1 sm:p-2">
						<TabButton
							activeView={activeView}
							onSelect={setActiveView}
							view="chat"
							icon={MessageSquare}
							label={t.chat}
						/>
						<TabButton
							activeView={activeView}
							onSelect={setActiveView}
							view="tasks"
							icon={ListTodo}
							label={t.tasks}
							badge={incompleteTasks}
						/>
						<TabButton
							activeView={activeView}
							onSelect={setActiveView}
							view="files"
							icon={FileText}
							label={t.files}
						/>
						<TabButton
							activeView={activeView}
							onSelect={setActiveView}
							view="canvas"
							icon={PaintBucket}
							label="Canvas"
						/>
						{features.mmry_enabled && (
							<TabButton
								activeView={activeView}
								onSelect={setActiveView}
								view="memories"
								icon={Brain}
								label={t.memories}
							/>
						)}
						<TabButton
							activeView={activeView}
							onSelect={setActiveView}
							view="terminal"
							icon={Terminal}
							label={t.terminal}
						/>
						<TabButton
							activeView={activeView}
							onSelect={setActiveView}
							view="settings"
							icon={Settings}
							label={locale === "de" ? "Einstellungen" : "Settings"}
						/>
					</div>
					{/* Mobile context window gauge - full width bar directly below tabs */}
					<ContextWindowGauge
						inputTokens={displayTokenUsage.inputTokens}
						outputTokens={displayTokenUsage.outputTokens}
						maxTokens={displayContextLimit}
						locale={locale}
						compact
					/>
				</div>

				{/* Mobile content */}
				<div
					className={cn(
						"flex-1 min-h-0 bg-card border border-t-0 border-border rounded-b-xl p-1.5 sm:p-4 overflow-hidden flex flex-col",
						activeView === "chat" && "pb-0",
					)}
				>
					{activeView === "chat" &&
						(mainChatActive ? (
							<MainChatPiView
								locale={locale}
								className="flex-1"
								features={features}
								workspacePath={mainChatWorkspacePath}
								assistantName={mainChatAssistantName}
								hideHeader
								onTokenUsageChange={setMainChatTokenUsage}
								selectedSessionId={mainChatCurrentSessionId}
								onSelectedSessionIdChange={setMainChatCurrentSessionId}
								scrollToMessageId={scrollToMessageId}
								onScrollToMessageComplete={() => setScrollToMessageId(null)}
								newSessionTrigger={mainChatNewSessionTrigger}
								onMessageSent={notifyMainChatSessionActivity}
								onTodosChange={setMainChatTodos}
							/>
						) : (
							renderChatContent(true)
						))}
					<div className={cn("h-full", activeView !== "files" && "hidden")}>
						{filesView}
					</div>
					{activeView === "tasks" && (
						<div className="flex flex-col h-full overflow-hidden">
							{/* Sub-tabs for Todos and Planner */}
							<div className="flex-shrink-0 flex border-b border-border bg-muted/30">
								<button
									type="button"
									onClick={() => setTasksSubTab("todos")}
									className={cn(
										"flex-1 px-3 py-2 text-xs font-medium transition-colors",
										tasksSubTab === "todos"
											? "text-foreground border-b-2 border-primary bg-background"
											: "text-muted-foreground hover:text-foreground hover:bg-muted/50",
									)}
								>
									<div className="flex items-center justify-center gap-1.5">
										<ListTodo className="w-3.5 h-3.5" />
										<span>Todos</span>
										{latestTodos.length > 0 && (
											<span className="text-[10px] px-1.5 py-0.5 bg-muted rounded-full">
												{latestTodos.length}
											</span>
										)}
									</div>
								</button>
								<button
									type="button"
									onClick={() => setTasksSubTab("planner")}
									className={cn(
										"flex-1 px-3 py-2 text-xs font-medium transition-colors",
										tasksSubTab === "planner"
											? "text-foreground border-b-2 border-primary bg-background"
											: "text-muted-foreground hover:text-foreground hover:bg-muted/50",
									)}
								>
									<div className="flex items-center justify-center gap-1.5">
										<CircleDot className="w-3.5 h-3.5" />
										<span>Planner</span>
									</div>
								</button>
							</div>

							{/* Tab content */}
							{tasksSubTab === "todos" && (
								<div className="flex-1 min-h-0 overflow-hidden">
									<TodoListView
										todos={latestTodos}
										emptyMessage={t.noTasks}
										fullHeight
									/>
								</div>
							)}
							{tasksSubTab === "planner" && (
								<TrxView
									key={resumeWorkspacePath ?? "no-workspace"}
									workspacePath={resumeWorkspacePath}
									className="flex-1 min-h-0"
									onStartIssue={(issueId, title, description) => {
										const content = description
											? `Working on #${issueId}: ${title}\n\n${description}\n\n`
											: `Working on #${issueId}: ${title}\n\n`;
										setMessageInputWithResize(content);
										// On mobile, switch to chat view
										if (window.innerWidth < 768) {
											setActiveView("chat");
										}
									}}
									onStartIssueNewSession={async (
										issueIds,
										title,
										attachments,
									) => {
										if (!resumeWorkspacePath) return;
										try {
											const url =
												await ensureOpencodeRunning(resumeWorkspacePath);
											if (!url) return;
											const newSession = await createSession(
												url,
												`${title}`,
												undefined,
												{ directory: resumeWorkspacePath },
											);
											await refreshOpencodeSessions();
											await refreshChatHistory();
											if (newSession.id) {
												setSelectedChatSessionId(newSession.id);
												setIssueAttachments(attachments);
												setActiveView("chat");
											}
										} catch (err) {
											console.error(
												"Failed to start issue in new session:",
												err,
											);
										}
									}}
									onAddIssueAttachments={(attachments) => {
										setIssueAttachments((prev) => [...prev, ...attachments]);
										setActiveView("chat");
									}}
								/>
							)}
						</div>
					)}
					{features.mmry_enabled && activeView === "memories" && (
						<Suspense fallback={viewLoadingFallback}>
							<MemoriesView
								workspacePath={resumeWorkspacePath}
								storeName={null}
							/>
						</Suspense>
					)}
					{activeView === "settings" && (
						<Suspense fallback={viewLoadingFallback}>
							{mainChatActive || isWorkspacePiSession ? (
								<PiSettingsView
									locale={locale}
									scope={mainChatActive ? "main" : "workspace"}
									sessionId={
										mainChatActive
											? mainChatCurrentSessionId
											: selectedChatSessionId
									}
									workspacePath={
										mainChatActive ? mainChatWorkspacePath : workspacePiPath
									}
								/>
							) : (
								<AgentSettingsView
									modelOptions={opencodeModelOptions}
									selectedModelRef={selectedModelRef}
									onModelChange={setSelectedModelRef}
									isModelLoading={isModelLoading}
								/>
							)}
						</Suspense>
					)}
					{activeView === "canvas" && (
						<div className="flex flex-col h-full overflow-hidden">
							{!isMobileLayout && (
								<div className="flex items-center justify-between px-2 py-1 border-b border-border bg-muted/30">
									<span className="text-xs text-muted-foreground">Canvas</span>
									<button
										type="button"
										onClick={() => toggleExpandedView("canvas")}
										className="p-1 text-muted-foreground hover:text-foreground hover:bg-muted/50"
										aria-label={
											expandedView === "canvas"
												? "Collapse canvas"
												: "Expand canvas"
										}
									>
										{expandedView === "canvas" ? (
											<Minimize2 className="w-3.5 h-3.5" />
										) : (
											<Maximize2 className="w-3.5 h-3.5" />
										)}
									</button>
								</div>
							)}
							<div className="flex-1 min-h-0">
								<Suspense fallback={viewLoadingFallback}>
									<CanvasView
										workspacePath={resumeWorkspacePath}
										initialImagePath={previewFilePath}
										onSaveAndAddToChat={handleCanvasSaveAndAddToChat}
									/>
								</Suspense>
							</div>
						</div>
					)}
					{/* Only mount the terminal when visible (terminal rendering is expensive). */}
					{isMobileLayout && activeView === "terminal" && (
						<div className="h-full">
							<Suspense fallback={viewLoadingFallback}>
								<TerminalView workspacePath={resumeWorkspacePath} />
							</Suspense>
						</div>
					)}
				</div>
			</div>

			{/* Desktop layout: side by side */}
			<div className="hidden lg:flex flex-1 min-h-0 gap-4 items-start">
				{/* Chat panel */}
				<div className="flex-[3] min-w-0 bg-card border border-border p-4 xl:p-6 flex flex-col min-h-0 h-full relative">
					{/* Top toolbar: search + sidebar collapse */}
					<div className="absolute top-4 right-4 xl:top-6 xl:right-6 flex items-center gap-1 z-10">
						<button
							type="button"
							onClick={() => setIsSearchOpen((prev) => !prev)}
							className="p-1.5 text-muted-foreground hover:text-foreground hover:bg-muted/50 rounded transition-colors"
							title={isSearchOpen ? "Close search (Esc)" : "Search (Ctrl+F)"}
						>
							{isSearchOpen ? (
								<X className="w-4 h-4" />
							) : (
								<Search className="w-4 h-4" />
							)}
						</button>
						<button
							type="button"
							onClick={() => setRightSidebarCollapsed((prev) => !prev)}
							className="p-1.5 text-muted-foreground hover:text-foreground hover:bg-muted/50 rounded transition-colors"
							title={
								rightSidebarCollapsed ? "Expand sidebar" : "Collapse sidebar"
							}
						>
							{rightSidebarCollapsed ? (
								<PanelLeftClose className="w-4 h-4" />
							) : (
								<PanelRightClose className="w-4 h-4" />
							)}
						</button>
					</div>
					{/* Search bar - shown when search is open */}
					{isSearchOpen && (
						<div className="mb-3 pr-16">
							<ChatSearchBar
								sessionId={
									mainChatActive
										? mainChatCurrentSessionId
										: selectedChatSessionId
								}
								onResultSelect={handleSearchResult}
								isOpen={isSearchOpen}
								onToggle={() => setIsSearchOpen(false)}
								locale={locale}
								hideCloseButton
							/>
						</div>
					)}
					{!mainChatActive && SessionHeader}
					{mainChatActive ? (
						expandedView ? (
							expandedPanel
						) : (
							<MainChatPiView
								locale={locale}
								className="flex-1"
								features={features}
								workspacePath={mainChatWorkspacePath}
								assistantName={mainChatAssistantName}
								onTokenUsageChange={setMainChatTokenUsage}
								selectedSessionId={mainChatCurrentSessionId}
								onSelectedSessionIdChange={setMainChatCurrentSessionId}
								scrollToMessageId={scrollToMessageId}
								onScrollToMessageComplete={() => setScrollToMessageId(null)}
								newSessionTrigger={mainChatNewSessionTrigger}
								onMessageSent={notifyMainChatSessionActivity}
								onTodosChange={setMainChatTodos}
							/>
						)
					) : chatInSidebar ? (
						expandedPanel
					) : (
						renderChatContent(true)
					)}
				</div>

				{/* Sidebar panel - collapsible */}
				<div
					className={cn(
						"bg-card border border-border flex flex-col min-h-0 h-full transition-all duration-200",
						rightSidebarCollapsed
							? "w-12 items-center"
							: "flex-[2] min-w-[320px] max-w-[420px]",
					)}
				>
					{rightSidebarCollapsed ? (
						/* Collapsed sidebar - vertical icon strip */
						<div className="flex flex-col gap-1 p-2 h-full overflow-y-auto">
							<CollapsedTabButton
								activeView={activeView}
								onSelect={(view) => {
									setActiveView(view);
									setRightSidebarCollapsed(false);
								}}
								view="tasks"
								icon={ListTodo}
								label={t.tasks}
								badge={incompleteTasks}
							/>
							<CollapsedTabButton
								activeView={activeView}
								onSelect={(view) => {
									setActiveView(view);
									setRightSidebarCollapsed(false);
								}}
								view="files"
								icon={FileText}
								label={t.files}
							/>
							<CollapsedTabButton
								activeView={activeView}
								onSelect={(view) => {
									setActiveView(view);
									setRightSidebarCollapsed(false);
								}}
								view="canvas"
								icon={PaintBucket}
								label="Canvas"
							/>
							{features.mmry_enabled && (
								<CollapsedTabButton
									activeView={activeView}
									onSelect={(view) => {
										setActiveView(view);
										setRightSidebarCollapsed(false);
									}}
									view="memories"
									icon={Brain}
									label={t.memories}
								/>
							)}
							<CollapsedTabButton
								activeView={activeView}
								onSelect={(view) => {
									setActiveView(view);
									setRightSidebarCollapsed(false);
								}}
								view="terminal"
								icon={Terminal}
								label={t.terminal}
							/>
							{voiceMode.isActive && features.voice && (
								<CollapsedTabButton
									activeView={activeView}
									onSelect={(view) => {
										setActiveView(view);
										setRightSidebarCollapsed(false);
									}}
									view="voice"
									icon={Mic}
									label="Voice"
								/>
							)}
							<CollapsedTabButton
								activeView={activeView}
								onSelect={(view) => {
									setActiveView(view);
									setRightSidebarCollapsed(false);
								}}
								view="settings"
								icon={Settings}
								label={locale === "de" ? "Einstellungen" : "Settings"}
							/>
						</div>
					) : (
						/* Expanded sidebar */
						<>
							{chatInSidebar ? (
								<div className="flex flex-col h-full min-h-0">
									<div className="flex items-center justify-between px-2 py-1 border-b border-border bg-muted/30">
										<span className="text-xs text-muted-foreground">
											{t.chat}
										</span>
										<button
											type="button"
											onClick={() => setExpandedView(null)}
											className="p-1 text-muted-foreground hover:text-foreground hover:bg-muted/50"
											aria-label="Return chat to main panel"
										>
											<Minimize2 className="w-3.5 h-3.5" />
										</button>
									</div>
									<div className="sidebar-chat flex-1 min-h-0 overflow-hidden flex flex-col">
										{mainChatActive ? (
											<MainChatPiView
												locale={locale}
												className="flex-1"
												features={features}
												workspacePath={mainChatWorkspacePath}
												assistantName={mainChatAssistantName}
												hideHeader
												onTokenUsageChange={setMainChatTokenUsage}
												selectedSessionId={mainChatCurrentSessionId}
												onSelectedSessionIdChange={setMainChatCurrentSessionId}
												scrollToMessageId={scrollToMessageId}
												onScrollToMessageComplete={() =>
													setScrollToMessageId(null)
												}
												newSessionTrigger={mainChatNewSessionTrigger}
												onMessageSent={notifyMainChatSessionActivity}
												onTodosChange={setMainChatTodos}
											/>
										) : (
											renderChatContent(false)
										)}
									</div>
								</div>
							) : (
								<>
									<div className="flex gap-1 p-2 border-b border-border">
										<TabButton
											activeView={activeView}
											onSelect={setActiveView}
											view="tasks"
											icon={ListTodo}
											label={t.tasks}
											badge={incompleteTasks}
											hideLabel
										/>
										<TabButton
											activeView={activeView}
											onSelect={setActiveView}
											view="files"
											icon={FileText}
											label={t.files}
											hideLabel
										/>
										<TabButton
											activeView={activeView}
											onSelect={setActiveView}
											view="canvas"
											icon={PaintBucket}
											label="Canvas"
											hideLabel
										/>
										{features.mmry_enabled && (
											<TabButton
												activeView={activeView}
												onSelect={setActiveView}
												view="memories"
												icon={Brain}
												label={t.memories}
												hideLabel
											/>
										)}
										<TabButton
											activeView={activeView}
											onSelect={setActiveView}
											view="terminal"
											icon={Terminal}
											label={t.terminal}
											hideLabel
										/>
										{voiceMode.isActive && features.voice && (
											<TabButton
												activeView={activeView}
												onSelect={setActiveView}
												view="voice"
												icon={Mic}
												label="Voice"
												hideLabel
											/>
										)}
										<TabButton
											activeView={activeView}
											onSelect={setActiveView}
											view="settings"
											icon={Settings}
											label={locale === "de" ? "Einstellungen" : "Settings"}
											hideLabel
										/>
									</div>
									<div className="flex-1 min-h-0 overflow-hidden">
										<div
											className={cn(
												"h-full",
												activeView !== "files" && "hidden",
											)}
										>
											{filesView}
										</div>
										{(activeView === "tasks" || activeView === "chat") && (
											<div className="flex flex-col h-full overflow-hidden">
												{/* Sub-tabs for Todos and Planner */}
												<div className="flex-shrink-0 flex border-b border-border bg-muted/30">
													<button
														type="button"
														onClick={() => setTasksSubTab("todos")}
														className={cn(
															"flex-1 px-3 py-2 text-xs font-medium transition-colors",
															tasksSubTab === "todos"
																? "text-foreground border-b-2 border-primary bg-background"
																: "text-muted-foreground hover:text-foreground hover:bg-muted/50",
														)}
													>
														<div className="flex items-center justify-center gap-1.5">
															<ListTodo className="w-3.5 h-3.5" />
															<span>Todos</span>
															{latestTodos.length > 0 && (
																<span className="text-[10px] px-1.5 py-0.5 bg-muted rounded-full">
																	{latestTodos.length}
																</span>
															)}
														</div>
													</button>
													<button
														type="button"
														onClick={() => setTasksSubTab("planner")}
														className={cn(
															"flex-1 px-3 py-2 text-xs font-medium transition-colors",
															tasksSubTab === "planner"
																? "text-foreground border-b-2 border-primary bg-background"
																: "text-muted-foreground hover:text-foreground hover:bg-muted/50",
														)}
													>
														<div className="flex items-center justify-center gap-1.5">
															<CircleDot className="w-3.5 h-3.5" />
															<span>Planner</span>
														</div>
													</button>
												</div>

												{/* Tab content */}
												{tasksSubTab === "todos" && (
													<div className="flex-1 min-h-0 overflow-hidden">
														<TodoListView
															todos={latestTodos}
															emptyMessage={t.noTasks}
															fullHeight
														/>
													</div>
												)}
												{tasksSubTab === "planner" && (
													<TrxView
														key={resumeWorkspacePath ?? "no-workspace"}
														workspacePath={resumeWorkspacePath}
														className="flex-1 min-h-0"
														onStartIssue={(issueId, title, description) => {
															const content = description
																? `Working on #${issueId}: ${title}\n\n${description}\n\n`
																: `Working on #${issueId}: ${title}\n\n`;
															setMessageInputWithResize(content);
															setActiveView("chat");
														}}
														onStartIssueNewSession={async (
															issueIds,
															title,
															attachments,
														) => {
															if (!resumeWorkspacePath) return;
															try {
																const url =
																	await ensureOpencodeRunning(
																		resumeWorkspacePath,
																	);
																if (!url) return;
																const newSession = await createSession(
																	url,
																	`${title}`,
																	undefined,
																	{ directory: resumeWorkspacePath },
																);
																await refreshOpencodeSessions();
																await refreshChatHistory();
																if (newSession.id) {
																	setSelectedChatSessionId(newSession.id);
																	setIssueAttachments(attachments);
																	setActiveView("chat");
																}
															} catch (err) {
																console.error(
																	"Failed to start issue in new session:",
																	err,
																);
															}
														}}
														onAddIssueAttachments={(attachments) => {
															setIssueAttachments((prev) => [
																...prev,
																...attachments,
															]);
															setActiveView("chat");
														}}
													/>
												)}
											</div>
										)}
										{features.mmry_enabled && activeView === "memories" && (
											<div className="flex flex-col h-full overflow-hidden">
												{!isMobileLayout && (
													<div className="flex items-center justify-between px-2 py-1 border-b border-border bg-muted/30">
														<span className="text-xs text-muted-foreground">
															{t.memories}
														</span>
														<button
															type="button"
															onClick={() => toggleExpandedView("memories")}
															className="p-1 text-muted-foreground hover:text-foreground hover:bg-muted/50"
															aria-label={
																expandedView === "memories"
																	? "Collapse memories"
																	: "Expand memories"
															}
														>
															{expandedView === "memories" ? (
																<Minimize2 className="w-3.5 h-3.5" />
															) : (
																<Maximize2 className="w-3.5 h-3.5" />
															)}
														</button>
													</div>
												)}
												<div className="flex-1 min-h-0">
													<Suspense fallback={viewLoadingFallback}>
														<MemoriesView
															workspacePath={resumeWorkspacePath}
															storeName={null}
														/>
													</Suspense>
												</div>
											</div>
										)}
										{activeView === "voice" && voiceMode.isActive && (
											<VoicePanel {...voicePanelProps} />
										)}
										{activeView === "settings" && (
											<Suspense fallback={viewLoadingFallback}>
												{mainChatActive || isWorkspacePiSession ? (
													<PiSettingsView
														locale={locale}
														scope={mainChatActive ? "main" : "workspace"}
														sessionId={
															mainChatActive
																? mainChatCurrentSessionId
																: selectedChatSessionId
														}
														workspacePath={
															mainChatActive
																? mainChatWorkspacePath
																: workspacePiPath
														}
													/>
												) : (
													<AgentSettingsView
														modelOptions={opencodeModelOptions}
														selectedModelRef={selectedModelRef}
														onModelChange={setSelectedModelRef}
														isModelLoading={isModelLoading}
													/>
												)}
											</Suspense>
										)}
										{activeView === "canvas" && (
											<div className="flex flex-col h-full overflow-hidden">
												{!isMobileLayout && (
													<div className="flex items-center justify-between px-2 py-1 border-b border-border bg-muted/30">
														<span className="text-xs text-muted-foreground">
															Canvas
														</span>
														<button
															type="button"
															onClick={() => toggleExpandedView("canvas")}
															className="p-1 text-muted-foreground hover:text-foreground hover:bg-muted/50"
															aria-label={
																expandedView === "canvas"
																	? "Collapse canvas"
																	: "Expand canvas"
															}
														>
															{expandedView === "canvas" ? (
																<Minimize2 className="w-3.5 h-3.5" />
															) : (
																<Maximize2 className="w-3.5 h-3.5" />
															)}
														</button>
													</div>
												)}
												<div className="flex-1 min-h-0">
													<Suspense fallback={viewLoadingFallback}>
														<CanvasView
															workspacePath={resumeWorkspacePath}
															initialImagePath={previewFilePath}
															onSaveAndAddToChat={handleCanvasSaveAndAddToChat}
														/>
													</Suspense>
												</div>
											</div>
										)}
										{/* Only mount the terminal when visible (terminal rendering is expensive). */}
										{!isMobileLayout && activeView === "terminal" && (
											<div className="h-full">
												<div className="flex flex-col h-full overflow-hidden">
													<div className="flex items-center justify-between px-2 py-1 border-b border-border bg-muted/30">
														<span className="text-xs text-muted-foreground">
															{t.terminal}
														</span>
														<button
															type="button"
															onClick={() => toggleExpandedView("terminal")}
															className="p-1 text-muted-foreground hover:text-foreground hover:bg-muted/50"
															aria-label={
																expandedView === "terminal"
																	? "Collapse terminal"
																	: "Expand terminal"
															}
														>
															{expandedView === "terminal" ? (
																<Minimize2 className="w-3.5 h-3.5" />
															) : (
																<Maximize2 className="w-3.5 h-3.5" />
															)}
														</button>
													</div>
													<div className="flex-1 min-h-0">
														<Suspense fallback={viewLoadingFallback}>
															<TerminalView
																workspacePath={resumeWorkspacePath}
															/>
														</Suspense>
													</div>
												</div>
											</div>
										)}
									</div>
								</>
							)}
						</>
					)}
				</div>
			</div>

			{/* Permission dialog */}
			<PermissionDialog
				permission={activePermission}
				onRespond={handlePermissionResponse}
				onDismiss={handlePermissionDismiss}
			/>

			{/* User question dialog */}
			<UserQuestionDialog
				request={activeQuestion}
				onReply={handleQuestionReply}
				onReject={handleQuestionReject}
				onDismiss={handleQuestionDismiss}
			/>

			{/* Voice mode overlay - mobile only */}
			{mobileVoiceOverlay}
		</div>
	);

	return perfEnabled ? (
		<Profiler id="SessionScreen" onRender={onProfilerRender}>
			{app}
		</Profiler>
	) : (
		app
	);
});

const MessageGroupCard = memo(function MessageGroupCard({
	group,
	persona,
	workspaceName,
	readableId,
	workspaceDirectory,
	onFork,
	locale = "en",
	a2uiSurfaces = [],
	onA2UIAction,
	messageId,
	showWorkingIndicator = false,
}: {
	group: MessageGroup;
	persona?: Persona | null;
	workspaceName?: string | null;
	readableId?: string | null;
	workspaceDirectory?: string;
	onFork?: (messageId: string) => void;
	locale?: "de" | "en";
	a2uiSurfaces?: A2UISurfaceState[];
	onA2UIAction?: (action: A2UIUserAction) => void;
	messageId?: string;
	showWorkingIndicator?: boolean;
}) {
	const isUser = group.role === "user";

	// Get the last message ID in the group (for forking from this point)
	const lastMessage = group.messages[group.messages.length - 1];
	const lastMessageId = lastMessage?.info.id;

	// Get created time from first message
	const firstMessage = group.messages[0];
	const createdAt = firstMessage.info.time?.created
		? new Date(firstMessage.info.time.created)
		: null;

	// Get all parts from all messages in order, preserving their sequence
	const allParts = group.messages.flatMap((msg) => msg.parts);

	// Group consecutive text parts together, but keep tool, file, and A2UI parts separate
	// This creates "segments" that maintain the original order
	type Segment =
		| { key: string; type: "text"; content: string; timestamp?: number }
		| { key: string; type: "tool"; part: OpenCodePart; timestamp?: number }
		| { key: string; type: "file"; part: OpenCodePart; timestamp?: number }
		| {
				key: string;
				type: "a2ui";
				surface: (typeof a2uiSurfaces)[0];
				timestamp: number;
		  }
		| { key: string; type: "other"; part: OpenCodePart; timestamp?: number };

	const segments: Segment[] = [];
	let currentTextBuffer: string[] = [];
	let currentTextKeys: string[] = [];
	let lastTimestamp = 0;

	const flushTextBuffer = () => {
		if (currentTextBuffer.length > 0) {
			const key = currentTextKeys[0] ?? `text-${segments.length}`;
			segments.push({
				key,
				type: "text",
				content: currentTextBuffer.join("\n\n"),
				timestamp: lastTimestamp,
			});
			currentTextBuffer = [];
			currentTextKeys = [];
		}
	};

	for (const [index, part] of allParts.entries()) {
		const partKey = part.id ?? `${part.type}-${index}`;
		// Get timestamp from tool state if available
		const partTimestamp =
			part.type === "tool" && part.state?.time?.start
				? part.state.time.start
				: lastTimestamp + index;
		lastTimestamp = partTimestamp;

		if (part.type === "text" && typeof part.text === "string") {
			// Skip text that looks like raw question tool JSON output
			// (questions array with header/options structure)
			const trimmedText = part.text.trim();
			const looksLikeQuestionJson =
				trimmedText.startsWith("{") &&
				trimmedText.includes('"questions"') &&
				trimmedText.includes('"header"') &&
				trimmedText.includes('"options"');
			if (!looksLikeQuestionJson) {
				currentTextBuffer.push(part.text);
				currentTextKeys.push(partKey);
			}
		} else if (part.type === "tool") {
			flushTextBuffer();
			segments.push({
				key: partKey,
				type: "tool",
				part,
				timestamp: partTimestamp,
			});
		} else if (part.type === "file") {
			flushTextBuffer();
			segments.push({
				key: partKey,
				type: "file",
				part,
				timestamp: partTimestamp,
			});
		} else {
			flushTextBuffer();
			segments.push({
				key: partKey,
				type: "other",
				part,
				timestamp: partTimestamp,
			});
		}
	}
	flushTextBuffer();

	// Insert A2UI surfaces into segments based on their creation timestamp
	for (const surface of a2uiSurfaces) {
		const surfaceTimestamp = surface.createdAt.getTime();
		segments.push({
			key: `a2ui-${surface.surfaceId}`,
			type: "a2ui",
			surface,
			timestamp: surfaceTimestamp,
		});
	}

	// Sort segments by timestamp to interleave A2UI with tool calls
	segments.sort((a, b) => (a.timestamp || 0) - (b.timestamp || 0));

	// Get all text content for copy button
	const allTextContent = allParts
		.filter(
			(p): p is OpenCodePart & { type: "text"; text: string } =>
				p.type === "text" && typeof p.text === "string",
		)
		.map((p) => p.text)
		.join("\n\n");

	// Get assistant display name: workspace name, persona name, or default
	const assistantDisplayName = workspaceName || persona?.name || "Assistant";
	const personaColor = persona?.color;

	const messageCard = (
		<div
			data-message-id={messageId}
			className={cn(
				"group transition-all duration-200 overflow-hidden",
				isUser
					? "sm:ml-8 bg-primary/20 dark:bg-primary/10 border border-primary/40 dark:border-primary/30"
					: "sm:mr-8 bg-muted/50 border border-border",
			)}
			style={
				!isUser && personaColor
					? { borderLeftColor: personaColor, borderLeftWidth: "3px" }
					: undefined
			}
		>
			{/* Header */}
			<div
				className={cn(
					"compact-header flex items-center gap-1 sm:gap-2 px-2 sm:px-3 py-1.5 sm:py-2 border-b",
					isUser ? "border-primary/30 dark:border-primary/20" : "border-border",
				)}
			>
				{isUser ? (
					<User className="w-3 h-3 sm:w-4 sm:h-4 text-primary flex-shrink-0" />
				) : personaColor ? (
					<div
						className="w-3 h-3 sm:w-4 sm:h-4 rounded-full flex-shrink-0"
						style={{ backgroundColor: personaColor }}
					/>
				) : (
					<Bot className="w-3 h-3 sm:w-4 sm:h-4 text-primary flex-shrink-0" />
				)}
				{isUser ? (
					<span className="text-sm font-medium text-foreground">You</span>
				) : (
					<span className="text-sm font-medium text-foreground">
						{assistantDisplayName}
						{readableId && (
							<span className="text-[9px] text-muted-foreground/70 ml-1">
								[{readableId}]
							</span>
						)}
					</span>
				)}
				{group.messages.length > 1 && (
					<span
						className={cn(
							"text-[9px] sm:text-[10px] px-1 border leading-none",
							isUser
								? "border-primary/30 text-primary"
								: "border-border text-muted-foreground",
						)}
					>
						{group.messages.length}
					</span>
				)}
				<div className="flex-1" />
				{/* Read aloud button for assistant messages */}
				{!isUser && allTextContent && (
					<ReadAloudButton text={allTextContent} className="ml-1" />
				)}
				{/* Timestamp */}
				{createdAt && !Number.isNaN(createdAt.getTime()) && (
					<span className="text-[9px] sm:text-[10px] text-foreground/50 dark:text-muted-foreground leading-none sm:leading-normal ml-2">
						{createdAt.toLocaleTimeString([], {
							hour: "2-digit",
							minute: "2-digit",
						})}
					</span>
				)}
				{/* Copy button - full size on desktop, compact on mobile */}
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

			{/* Content - render segments in order */}
			<div className="px-2 sm:px-4 py-2 sm:py-3 group space-y-3 overflow-hidden">
				{segments.length === 0 && !isUser && showWorkingIndicator && (
					<div className="flex items-center gap-3 text-muted-foreground text-sm">
						<BrailleSpinner />
						<span>Working...</span>
					</div>
				)}
				{segments.length === 0 && isUser && (
					<span className="text-muted-foreground italic text-sm">
						No content
					</span>
				)}

				{segments.map((segment, idx) => {
					// Add top margin to non-text segments that follow text segments
					const prevSegment = idx > 0 ? segments[idx - 1] : null;
					const needsTopMargin =
						prevSegment?.type === "text" && segment.type !== "text";

					if (segment.type === "text") {
						// Parse @file references from the text, excluding code blocks
						const uniqueFileRefs = extractFileReferences(segment.content);

						return (
							<ContextMenu key={segment.key}>
								<ContextMenuTrigger className="contents">
									<div className="overflow-hidden space-y-2 select-none sm:select-auto">
										<MarkdownRenderer
											content={segment.content}
											className="text-sm text-foreground leading-relaxed overflow-hidden"
										/>
										{uniqueFileRefs.map((filePath) => (
											<FileReferenceCard
												key={filePath}
												filePath={filePath}
												workspacePath={workspaceDirectory}
											/>
										))}
									</div>
								</ContextMenuTrigger>
								<ContextMenuContent>
									<ContextMenuItem
										onClick={() =>
											navigator.clipboard?.writeText(segment.content)
										}
										className="gap-2"
									>
										<Copy className="w-4 h-4" />
										{locale === "de" ? "Kopieren" : "Copy"}
									</ContextMenuItem>
								</ContextMenuContent>
							</ContextMenu>
						);
					}

					if (segment.type === "file") {
						return (
							<div
								key={segment.key}
								className={needsTopMargin ? "mt-3" : undefined}
							>
								<FilePartCard
									part={segment.part}
									workspaceDirectory={workspaceDirectory}
								/>
							</div>
						);
					}

					if (segment.type === "tool") {
						return (
							<div
								key={segment.key}
								className={needsTopMargin ? "mt-3" : undefined}
							>
								<ToolCallCard
									part={segment.part}
									defaultCollapsed={true}
									hideTodoTools={true}
								/>
							</div>
						);
					}

					if (segment.type === "other") {
						return (
							<div
								key={segment.key}
								className={needsTopMargin ? "mt-3" : undefined}
							>
								<OtherPartCard part={segment.part} />
							</div>
						);
					}

					if (segment.type === "a2ui") {
						return (
							<div
								key={segment.key}
								className={needsTopMargin ? "mt-3" : undefined}
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

	// Always wrap in context menu for copy all (and fork if available)
	return (
		<ContextMenu>
			<ContextMenuTrigger className="contents">
				{messageCard}
			</ContextMenuTrigger>
			<ContextMenuContent>
				{allTextContent && (
					<ContextMenuItem
						onClick={() => navigator.clipboard?.writeText(allTextContent)}
						className="gap-2"
					>
						<Copy className="w-4 h-4" />
						{locale === "de" ? "Alles kopieren" : "Copy all"}
					</ContextMenuItem>
				)}
				{onFork && lastMessageId && (
					<>
						{allTextContent && <ContextMenuSeparator />}
						<ContextMenuItem
							onClick={() => onFork(lastMessageId)}
							className="gap-2"
						>
							<GitBranch className="w-4 h-4" />
							{locale === "de" ? "Von hier verzweigen" : "Branch from here"}
						</ContextMenuItem>
					</>
				)}
			</ContextMenuContent>
		</ContextMenu>
	);
});

const OtherPartCard = memo(function OtherPartCard({
	part,
}: { part: OpenCodePart }) {
	const [isOpen, setIsOpen] = useState(false);

	const getPartLabel = () => {
		switch (part.type) {
			case "reasoning":
				return "Reasoning";
			case "file":
				return "File";
			case "snapshot":
				return "Snapshot";
			case "patch":
				return "Patch";
			case "agent":
				return "Agent";
			case "step-start":
				return "Step Start";
			case "step-finish":
				return "Step Finish";
			case "retry":
				return "Retry";
			case "compaction":
				return "Compaction";
			case "subtask":
				return "Subtask";
			default:
				return part.type;
		}
	};

	const content =
		part.text ||
		(part.metadata ? JSON.stringify(part.metadata, null, 2) : null);
	if (!content) return null;

	return (
		<div className="border border-border bg-muted/30 overflow-hidden">
			<button
				type="button"
				onClick={() => setIsOpen(!isOpen)}
				className="w-full flex items-center gap-2 px-3 py-2 text-left hover:bg-muted/50 transition-colors"
			>
				<ChevronDown
					className={cn(
						"w-4 h-4 text-muted-foreground transition-transform",
						isOpen && "rotate-180",
					)}
				/>
				<span className="text-xs uppercase tracking-wide text-muted-foreground">
					{getPartLabel()}
				</span>
			</button>

			{isOpen && (
				<div className="px-3 pb-3 border-t border-border">
					<pre className="text-xs text-muted-foreground mt-2 whitespace-pre-wrap overflow-x-auto">
						{content}
					</pre>
				</div>
			)}
		</div>
	);
});

type FilePartDetails = {
	filePath: string;
	fileName: string;
	directUrl?: string;
};

const extractFilePartDetails = (
	part: OpenCodePart,
	workspaceDirectory?: string,
): FilePartDetails | null => {
	const metadata = part.metadata ?? {};
	const rawUrl =
		part.url ||
		(typeof metadata.url === "string" ? metadata.url : undefined) ||
		null;
	const metadataPath =
		typeof metadata.path === "string"
			? metadata.path
			: typeof metadata.filePath === "string"
				? metadata.filePath
				: typeof metadata.file === "string"
					? metadata.file
					: null;
	const fileName =
		part.filename ||
		(typeof metadata.filename === "string" ? metadata.filename : undefined) ||
		metadataPath ||
		"file";

	if (typeof rawUrl === "string") {
		if (rawUrl.startsWith("file://")) {
			const absolutePath = decodeURIComponent(rawUrl.replace("file://", ""));
			if (workspaceDirectory && absolutePath.startsWith(workspaceDirectory)) {
				const relativePath = absolutePath
					.slice(workspaceDirectory.length)
					.replace(/^\/+/, "");
				return {
					filePath: relativePath || fileName,
					fileName,
				};
			}
			if (workspaceDirectory) {
				return { filePath: metadataPath || fileName, fileName };
			}
			return { filePath: absolutePath, fileName };
		}
		if (rawUrl.startsWith("http://") || rawUrl.startsWith("https://")) {
			return {
				filePath: metadataPath || fileName,
				fileName,
				directUrl: rawUrl,
			};
		}
	}

	if (metadataPath) {
		return { filePath: metadataPath, fileName };
	}

	return fileName ? { filePath: fileName, fileName } : null;
};

const FilePartCard = memo(function FilePartCard({
	part,
	workspaceDirectory,
}: {
	part: OpenCodePart;
	workspaceDirectory?: string;
}) {
	const details = useMemo(
		() => extractFilePartDetails(part, workspaceDirectory),
		[part, workspaceDirectory],
	);

	if (!details) {
		return <OtherPartCard part={part} />;
	}

	return (
		<FileReferenceCard
			filePath={details.filePath}
			workspacePath={workspaceDirectory}
			directUrl={details.directUrl}
			label={details.fileName}
		/>
	);
});

/** Renders a file reference card with preview for images.
 * Only renders if the file exists on disk (verified via HEAD request).
 */
const FileReferenceCard = memo(function FileReferenceCard({
	filePath,
	workspacePath,
	directUrl,
	label,
}: {
	filePath: string;
	workspacePath?: string | null;
	directUrl?: string;
	label?: string;
}) {
	const [isLoading, setIsLoading] = useState(true);
	const [error, setError] = useState<string | null>(null);
	const [imageLoaded, setImageLoaded] = useState(false);
	const [fileExists, setFileExists] = useState<boolean | null>(null);

	const fileInfo = useMemo(() => getFileTypeInfo(filePath), [filePath]);
	const isImage = fileInfo.category === "image";
	const isVideo = fileInfo.category === "video";
	const fileName = label || filePath.split("/").pop() || filePath;

	// Build the file URL
	const fileUrl = useMemo(() => {
		if (directUrl) return directUrl;
		if (!workspacePath) return null;
		return workspaceFileUrl(workspacePath, filePath);
	}, [directUrl, filePath, workspacePath]);

	// Check if file exists using HEAD request
	useEffect(() => {
		if (!fileUrl) {
			setFileExists(false);
			return;
		}
		let cancelled = false;
		fetch(fileUrl, {
			method: "HEAD",
			credentials: "include",
			headers: getAuthHeaders(),
		})
			.then((res) => {
				if (!cancelled) {
					setFileExists(res.ok);
					if (!res.ok) setIsLoading(false);
				}
			})
			.catch(() => {
				if (!cancelled) {
					setFileExists(false);
					setIsLoading(false);
				}
			});
		return () => {
			cancelled = true;
		};
	}, [fileUrl]);

	// Don't render if file doesn't exist or we're still checking
	if (fileExists === null || fileExists === false) {
		return null;
	}

	if (!fileUrl) {
		return null;
	}

	// For images, render inline preview
	if (isImage) {
		return (
			<div className="border border-border bg-muted/20 rounded overflow-hidden max-w-md">
				<div className="flex items-center gap-2 px-3 py-2 bg-muted/50 border-b border-border">
					<FileImage className="w-4 h-4 text-muted-foreground" />
					<span className="text-xs font-medium truncate">{fileName}</span>
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
							src={fileUrl}
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

	// For videos, render inline preview with player
	if (isVideo) {
		return (
			<div className="border border-border bg-muted/20 rounded overflow-hidden max-w-md">
				<div className="flex items-center gap-2 px-3 py-2 bg-muted/50 border-b border-border">
					<FileVideo className="w-4 h-4 text-muted-foreground" />
					<span className="text-xs font-medium truncate">{fileName}</span>
				</div>
				<video
					src={fileUrl}
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
		<a
			href={fileUrl}
			target="_blank"
			rel="noopener noreferrer"
			className="inline-flex items-center gap-2 px-3 py-1.5 border border-border bg-muted/20 rounded hover:bg-muted/40 transition-colors text-sm"
		>
			<FileIcon className="w-4 h-4 text-muted-foreground" />
			<span className="font-medium">{fileName}</span>
			<span className="text-xs text-muted-foreground">{filePath}</span>
		</a>
	);
});

const TodoListView = memo(function TodoListView({
	todos,
	emptyMessage,
	fullHeight = false,
}: { todos: TodoItem[]; emptyMessage: string; fullHeight?: boolean }) {
	// Group todos by status for summary
	const summary = useMemo(() => {
		const pending = todos.filter((t) => t.status === "pending").length;
		const inProgress = todos.filter((t) => t.status === "in_progress").length;
		const completed = todos.filter((t) => t.status === "completed").length;
		const cancelled = todos.filter((t) => t.status === "cancelled").length;
		return { pending, inProgress, completed, cancelled, total: todos.length };
	}, [todos]);

	if (todos.length === 0) {
		// Show empty state when in full height mode (tabbed view)
		if (fullHeight) {
			return (
				<div className="flex flex-col items-center justify-center h-full text-muted-foreground">
					<ListTodo className="w-8 h-8 mb-2 opacity-50" />
					<p className="text-sm">{emptyMessage}</p>
				</div>
			);
		}
		// Return null when empty to allow TrxView to take full space (stacked view)
		return null;
	}

	return (
		<div
			className={cn(
				"flex flex-col overflow-hidden",
				fullHeight ? "h-full" : "flex-shrink-0 max-h-[40%]",
			)}
			data-spotlight="todo-list"
		>
			{/* Summary header */}
			<div className="px-3 py-2 border-b border-border bg-muted/30">
				<div className="flex items-center justify-between text-xs">
					<span className="text-muted-foreground">{summary.total} tasks</span>
					<div className="flex items-center gap-3">
						{summary.inProgress > 0 && (
							<span className="flex items-center gap-1 text-primary">
								<CircleDot className="w-3 h-3" />
								{summary.inProgress}
							</span>
						)}
						{summary.pending > 0 && (
							<span className="flex items-center gap-1 text-muted-foreground">
								<Square className="w-3 h-3" />
								{summary.pending}
							</span>
						)}
						{summary.completed > 0 && (
							<span className="flex items-center gap-1 text-primary">
								<CheckSquare className="w-3 h-3" />
								{summary.completed}
							</span>
						)}
					</div>
				</div>
			</div>

			{/* Todo list */}
			<div className="flex-1 overflow-y-auto px-3 py-2 space-y-1">
				{todos.map((todo, idx) => (
					<div
						key={todo.id || idx}
						className={cn(
							"flex items-start gap-2 p-2 transition-colors",
							todo.status === "in_progress" &&
								"bg-primary/10 border border-primary/30",
							todo.status === "completed" && "opacity-50",
							todo.status === "cancelled" && "opacity-40",
							todo.status === "pending" && "bg-muted/30 border border-border",
						)}
					>
						{/* Status icon */}
						<div className="flex-shrink-0 mt-0.5">
							{todo.status === "completed" ? (
								<CheckSquare className="w-4 h-4 text-primary" />
							) : todo.status === "in_progress" ? (
								<CircleDot className="w-4 h-4 text-primary animate-pulse" />
							) : todo.status === "cancelled" ? (
								<XCircle className="w-4 h-4 text-muted-foreground" />
							) : (
								<Square className="w-4 h-4 text-muted-foreground" />
							)}
						</div>

						{/* Content */}
						<div className="flex-1 min-w-0">
							<p
								className={cn(
									"text-sm leading-relaxed",
									todo.status === "completed"
										? "text-muted-foreground line-through"
										: "text-foreground",
									todo.status === "cancelled" && "line-through",
								)}
							>
								{todo.content}
							</p>
						</div>

						{/* Priority badge */}
						{todo.priority && (
							<span
								className={cn(
									"text-[10px] uppercase tracking-wide flex-shrink-0 px-1.5 py-0.5",
									todo.priority === "high" && "bg-red-400/10 text-red-400",
									todo.priority === "medium" &&
										"bg-yellow-400/10 text-yellow-400",
									todo.priority === "low" && "bg-muted text-muted-foreground",
								)}
							>
								{todo.priority}
							</span>
						)}
					</div>
				))}
			</div>
		</div>
	);
});

export default SessionScreen;
