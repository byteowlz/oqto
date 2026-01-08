"use client";

import {
	type FileTreeState,
	FileTreeView,
	initialFileTreeState,
} from "@/apps/sessions/FileTreeView";
import { MainChatPiView } from "@/components/main-chat";
import { Badge } from "@/components/ui/badge";
import { BrailleSpinner } from "@/components/ui/braille-spinner";
import { Button } from "@/components/ui/button";
import { ContextWindowGauge } from "@/components/ui/context-window-gauge";
import {
	ContextMenu,
	ContextMenuContent,
	ContextMenuItem,
	ContextMenuSeparator,
	ContextMenuTrigger,
} from "@/components/ui/context-menu";
import {
	type FileAttachment,
	FileAttachmentChip,
	FileMentionPopup,
} from "@/components/ui/file-mention-popup";
import { Input } from "@/components/ui/input";
import {
	CopyButton,
	MarkdownRenderer,
} from "@/components/ui/markdown-renderer";
import {
	PermissionBanner,
	PermissionDialog,
} from "@/components/ui/permission-dialog";
import { ReadAloudButton } from "@/components/ui/read-aloud-button";
import {
	Select,
	SelectContent,
	SelectItem,
	SelectTrigger,
	SelectValue,
} from "@/components/ui/select";
import { SlashCommandPopup } from "@/components/ui/slash-command-popup";
import { ToolCallCard } from "@/components/ui/tool-call-card";
import {
	VoiceInputOverlay,
	VoiceMenuButton,
	type VoiceMode,
	VoicePanel,
} from "@/components/voice";
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
import {
	type Features,
	type MainChatSession,
	type Persona,
	type SessionAutoAttachMode,
	controlPlaneDirectBaseUrl,
	convertChatMessagesToOpenCode,
	fileserverWorkspaceBaseUrl,
	getChatMessages,
	getFeatures,
	getMainChatAssistant,
	getProjectLogoUrl,
	getWorkspaceConfig,
	listMainChatSessions,
	opencodeProxyBaseUrl,
	registerMainChatSession,
} from "@/lib/control-plane-client";
import { getFileTypeInfo } from "@/lib/file-types";
import {
	type OpenCodeAssistantMessage,
	type OpenCodeMessageWithParts,
	type OpenCodePart,
	type OpenCodePartInput,
	type Permission,
	type PermissionResponse,
	abortSession,
	createSession,
	fetchAgents,
	fetchCommands,
	fetchMessages,
	fetchProviders,
	fetchSessions,
	forkSession,
	invalidateMessageCache,
	respondToPermission,
	runShellCommandAsync,
	sendCommandAsync,
	sendMessageAsync,
	sendPartsAsync,
} from "@/lib/opencode-client";
import { formatSessionDate, generateReadableId } from "@/lib/session-utils";
import { type ModelOption, filterModelOptions } from "@/lib/model-filter";
import {
	type SlashCommand,
	builtInCommands,
	commandInfoToSlashCommands,
	parseSlashInput,
} from "@/lib/slash-commands";
import { cn } from "@/lib/utils";
import {
	ArrowDown,
	AudioLines,
	Bot,
	Brain,
	Check,
	CheckSquare,
	ChevronDown,
	CircleDot,
	Clock,
	Copy,
	Eye,
	FileCode,
	FileImage,
	FileText,
	GitBranch,
	ListTodo,
	Loader2,
	MessageSquare,
	Mic,
	PaintBucket,
	Paperclip,
	RefreshCw,
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
	Suspense,
	lazy,
	memo,
	startTransition,
	useCallback,
	useEffect,
	useLayoutEffect,
	useMemo,
	useRef,
	useState,
	useTransition,
} from "react";
import { toast } from "sonner";

const PreviewView = lazy(() =>
	import("@/apps/sessions/PreviewView").then((mod) => ({
		default: mod.PreviewView,
	})),
);
const TerminalView = lazy(() =>
	import("@/apps/sessions/TerminalView").then((mod) => ({
		default: mod.TerminalView,
	})),
);
const MemoriesView = lazy(() =>
	import("@/apps/sessions/MemoriesView").then((mod) => ({
		default: mod.MemoriesView,
	})),
);
const AgentSettingsView = lazy(() =>
	import("@/apps/sessions/AgentSettingsView").then((mod) => ({
		default: mod.AgentSettingsView,
	})),
);
const TrxView = lazy(() =>
	import("@/apps/sessions/TrxView").then((mod) => ({
		default: mod.TrxView,
	})),
);
const CanvasView = lazy(() =>
	import("@/apps/sessions/CanvasView").then((mod) => ({
		default: mod.CanvasView,
	})),
);

// Todo item structure
interface TodoItem {
	id: string;
	content: string;
	status: "pending" | "in_progress" | "completed" | "cancelled";
	priority: "high" | "medium" | "low";
}

// Extended message type for Main Chat threading - includes session info
type ThreadedMessage = OpenCodeMessageWithParts & {
	/** Session ID this message belongs to (for Main Chat threading) */
	_sessionId?: string;
	/** Session title (for displaying session dividers) */
	_sessionTitle?: string;
	/** Whether this is the first message of a new session in the thread */
	_isSessionStart?: boolean;
};

// Group consecutive messages from the same role
type MessageGroup = {
	role: "user" | "assistant";
	messages: OpenCodeMessageWithParts[];
	startIndex: number;
	/** For Main Chat: session ID this group belongs to */
	sessionId?: string;
	/** For Main Chat: whether this group starts a new session */
	isNewSession?: boolean;
	/** For Main Chat: session title for divider */
	sessionTitle?: string;
};

type ActiveView =
	| "chat"
	| "files"
	| "terminal"
	| "preview"
	| "tasks"
	| "memories"
	| "voice"
	| "settings"
	| "canvas";

function groupMessages(messages: OpenCodeMessageWithParts[]): MessageGroup[] {
	const groups: MessageGroup[] = [];
	let currentGroup: MessageGroup | null = null;
	let lastSessionId: string | undefined;

	messages.forEach((msg, index) => {
		const role = msg.info.role;
		const threadedMsg = msg as ThreadedMessage;
		const currentSessionId = threadedMsg._sessionId;
		const isNewSession =
			currentSessionId !== lastSessionId && currentSessionId !== undefined;

		// Start new group if role changes OR session changes (for Main Chat threading)
		if (
			!currentGroup ||
			currentGroup.role !== role ||
			(isNewSession && currentSessionId)
		) {
			if (currentGroup) {
				groups.push(currentGroup);
			}
			currentGroup = {
				role,
				messages: [msg],
				startIndex: index,
				sessionId: currentSessionId,
				isNewSession: isNewSession,
				sessionTitle: threadedMsg._sessionTitle,
			};
			lastSessionId = currentSessionId;
		} else {
			currentGroup.messages.push(msg);
		}
	});

	if (currentGroup) {
		groups.push(currentGroup);
	}

	return groups;
}

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

function parseModelRef(value: string): { providerID: string; modelID: string } | null {
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

function normalizePermissionEvent(value: unknown): Permission | null {
	if (!value || typeof value !== "object") return null;
	const record = value as Record<string, unknown>;
	const props =
		typeof record.properties === "object" && record.properties !== null
			? (record.properties as Record<string, unknown>)
			: record;
	const id =
		(typeof props.id === "string" && props.id) ||
		(typeof props.permissionID === "string" && props.permissionID) ||
		"";
	const type = typeof props.type === "string" ? props.type : "";
	if (!id || !type) return null;
	return {
		id,
		type,
		sessionID: typeof props.sessionID === "string" ? props.sessionID : "",
		title: typeof props.title === "string" ? props.title : "",
		pattern:
			typeof props.pattern === "string" || Array.isArray(props.pattern)
				? (props.pattern as Permission["pattern"])
				: undefined,
		metadata:
			typeof props.metadata === "object" && props.metadata !== null
				? (props.metadata as Record<string, unknown>)
				: {},
		time:
			typeof props.time === "object" && props.time !== null
				? (props.time as Permission["time"])
				: { created: Date.now() },
	};
}

export function SessionsApp() {
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
	} = useApp();
	const [messages, setMessages] = useState<OpenCodeMessageWithParts[]>([]);
	const [messageInput, setMessageInput] = useState("");
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

	const [opencodeModelOptions, setOpencodeModelOptions] = useState<ModelOption[]>(
		[],
	);
	const [selectedModelRef, setSelectedModelRef] = useState<string | null>(null);
	const [isModelLoading, setIsModelLoading] = useState(false);
	const [modelQuery, setModelQuery] = useState("");
	const modelStorageKey = useMemo(() => {
		if (!selectedWorkspaceSessionId || mainChatActive) return null;
		return `octo:opencodeModel:${selectedWorkspaceSessionId}`;
	}, [selectedWorkspaceSessionId, mainChatActive]);

	useEffect(() => {
		if (!modelStorageKey) {
			setSelectedModelRef(null);
			return;
		}
		const stored = localStorage.getItem(modelStorageKey);
		setSelectedModelRef(stored || null);
	}, [modelStorageKey]);

	useEffect(() => {
		if (!modelStorageKey) return;
		if (selectedModelRef) {
			localStorage.setItem(modelStorageKey, selectedModelRef);
		} else {
			localStorage.removeItem(modelStorageKey);
		}
	}, [modelStorageKey, selectedModelRef]);

	useEffect(() => {
		if (!effectiveOpencodeBaseUrl || mainChatActive) {
			setOpencodeModelOptions([]);
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
			.catch(() => {
				if (active) setOpencodeModelOptions([]);
			})
			.finally(() => {
				if (active) setIsModelLoading(false);
			});

		return () => {
			active = false;
		};
	}, [effectiveOpencodeBaseUrl, opencodeDirectory, mainChatActive]);

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
	const selectedModelOverride = useMemo(() => {
		if (!selectedModelRef) return undefined;
		return parseModelRef(selectedModelRef) ?? undefined;
	}, [selectedModelRef]);
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

	// Restore draft only when switching to a new session
	useEffect(() => {
		const prevId = previousSessionIdRef.current;
		const currId = selectedChatSessionId;

		// Restore draft for current session when switching (or clear if none)
		if (currId && currId !== prevId) {
			const savedDraft = getDraft(currId);
			setMessageInput(savedDraft);
		}

		previousSessionIdRef.current = currId;
	}, [selectedChatSessionId, getDraft]);

	// Auto-resize textarea when messageInput changes programmatically (e.g., draft restoration)
	useEffect(() => {
		if (chatInputRef.current) {
			const textarea = chatInputRef.current;
			if (!messageInput) {
				// No content - reset to minimum height
				textarea.style.height = "36px";
			} else {
				// Has content - calculate needed height
				textarea.style.height = "36px"; // Reset first to get accurate scrollHeight
				const scrollHeight = textarea.scrollHeight;
				textarea.style.height = `${Math.min(scrollHeight, 200)}px`;
			}
		}
	}, [messageInput]);

	const [isLoading, setIsLoading] = useState(true);
	const [showTimeoutError, setShowTimeoutError] = useState(false);
	const [activeView, setActiveView] = useState<ActiveView>("chat");
	const [status, setStatus] = useState<string>("");
	const [showScrollToBottom, setShowScrollToBottom] = useState(false);
	const [previewFilePath, setPreviewFilePath] = useState<string | null>(null);
	const [fileTreeState, setFileTreeState] =
		useState<FileTreeState>(initialFileTreeState);
	const messagesContainerRef = useRef<HTMLDivElement>(null);
	const messagesEndRef = useRef<HTMLDivElement>(null);

	// Track if auto-scroll is enabled (user hasn't scrolled away)
	const autoScrollEnabledRef = useRef(true);
	const lastSessionIdRef = useRef<string | null>(null);
	// Track last scroll position to detect scroll direction
	const lastScrollTopRef = useRef(0);
	// Track if this is initial message load (for instant scroll) vs streaming (for smooth scroll)
	const initialLoadRef = useRef(true);
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
	const fileInputRef = useRef<HTMLInputElement>(null);
	const chatInputRef = useRef<HTMLTextAreaElement>(null);
	const chatContainerRef = useRef<HTMLDivElement>(null);
	const prevVoiceActiveRef = useRef(false);

	// File upload state
	const [pendingUploads, setPendingUploads] = useState<
		{ name: string; path: string }[]
	>([]);

	// Slash command popup state
	const [showSlashPopup, setShowSlashPopup] = useState(false);
	const [slashCommands, setSlashCommands] =
		useState<SlashCommand[]>(builtInCommands);
	const slashQuery = parseSlashInput(messageInput);

	// File mention popup state
	const [showFileMentionPopup, setShowFileMentionPopup] = useState(false);
	const [fileMentionQuery, setFileMentionQuery] = useState("");
	const [fileAttachments, setFileAttachments] = useState<FileAttachment[]>([]);

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

	// Clear permission state when session changes
	// Note: Permissions are received via SSE events (permission.updated), not fetched via REST
	const prevSessionRef = useRef(selectedChatSessionId);
	useEffect(() => {
		if (prevSessionRef.current !== selectedChatSessionId) {
			prevSessionRef.current = selectedChatSessionId;
			setPendingPermissions([]);
			setActivePermission(null);
		}
	});

	// Track if we're on mobile layout (below lg breakpoint = 1024px)
	const isMobileLayout = useIsMobile();

	// Feature flags from backend
	const [features, setFeatures] = useState<Features>({ mmry_enabled: false });

	// Fetch features on mount
	useEffect(() => {
		getFeatures()
			.then(setFeatures)
			.catch(() => {
				// Silently ignore - features will remain disabled
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

	// Voice mode - handles STT/TTS when voice feature is enabled
	const handleVoiceTranscript = useCallback((text: string) => {
		// Set the transcript as message input and send it
		setMessageInput(text);
		// We'll trigger send after a small delay to allow state to update
		setTimeout(() => {
			const sendBtn = document.querySelector(
				"[data-voice-send]",
			) as HTMLButtonElement;
			if (sendBtn) sendBtn.click();
		}, 100);
	}, []);

	const voiceMode = useVoiceMode({
		config: features.voice ?? null,
		onTranscript: handleVoiceTranscript,
	});

	// Dictation mode - speech to text for the input field
	// Use a ref to track the current message input for dictation appending
	// This ensures dictation always appends to the latest value, even when user is typing
	const messageInputRef = useRef(messageInput);
	useEffect(() => {
		messageInputRef.current = messageInput;
	}, [messageInput]);

	const handleDictationTranscript = useCallback((text: string) => {
		// Always append to the current value using the ref to avoid stale closures
		const currentValue = messageInputRef.current;
		setMessageInput(currentValue ? `${currentValue} ${text}` : text);
	}, []);

	const dictation = useDictation({
		config: features.voice ?? null,
		onTranscript: handleDictationTranscript,
		vadTimeoutMs: 3000, // Longer timeout for dictation (3s silence before auto-stop appending)
	});

	// Auto-resize textarea during dictation based on liveTranscript length
	useEffect(() => {
		if (!chatInputRef.current || !dictation.isActive) return;

		const textarea = chatInputRef.current;
		const transcript = dictation.liveTranscript;

		if (!transcript) {
			textarea.style.height = "36px";
			return;
		}

		// Estimate height from transcript length (~50 chars per line at typical width)
		const estimatedLines = Math.ceil(transcript.length / 50);
		const estimatedHeight = Math.min(36 + (estimatedLines - 1) * 20, 200);
		textarea.style.height = `${estimatedHeight}px`;
	}, [dictation.isActive, dictation.liveTranscript]);

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

	// Streaming TTS: Track what text we've already sent to TTS for current message
	const ttsStreamStateRef = useRef<{
		messageId: string | null;
		sentLength: number; // How many characters we've already sent
	}>({ messageId: null, sentLength: 0 });

	// Auto-TTS: Stream assistant responses to TTS as text arrives
	// Kokorox handles sentence segmentation internally
	useEffect(() => {
		// Only trigger TTS when voice mode is active and not muted
		if (!voiceMode.isActive || voiceMode.settings.muted) return;

		// Find the last assistant message
		const lastMessage = messages[messages.length - 1];
		if (!lastMessage || lastMessage.info.role !== "assistant") return;

		const messageId = lastMessage.info.id;
		const streamState = ttsStreamStateRef.current;

		// Reset state if this is a new message
		if (streamState.messageId !== messageId) {
			streamState.messageId = messageId;
			streamState.sentLength = 0;
		}

		// Extract text content from the message parts
		const textParts = lastMessage.parts
			.filter(
				(p): p is OpenCodePart & { type: "text"; text: string } =>
					p.type === "text" && typeof p.text === "string",
			)
			.map((p) => p.text);

		if (textParts.length === 0) return;

		// Get full text so far
		const fullText = textParts.join("\n\n");

		// Nothing new to send
		if (fullText.length <= streamState.sentLength) return;

		// Get the new text since last send and stream it to kokorox
		const newText = fullText.slice(streamState.sentLength);
		streamState.sentLength = fullText.length;

		console.log(
			"[Voice] Streaming TTS:",
			newText.slice(0, 50) + (newText.length > 50 ? "..." : ""),
		);
		voiceMode.speak(newText).catch((err) => {
			console.error("[Voice] Auto-TTS failed:", err);
		});
	}, [messages, voiceMode.isActive, voiceMode.settings.muted, voiceMode.speak]);

	// Stop TTS playback when voice mode is deactivated
	useEffect(() => {
		if (!voiceMode.isActive) {
			voiceMode.interrupt();
			// Reset stream state so next activation starts fresh
			ttsStreamStateRef.current = { messageId: null, sentLength: 0 };
		}
	}, [voiceMode.isActive, voiceMode.interrupt]);

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
		setActiveView("preview");
	}, []);

	// Handler for opening a file in canvas from FileTreeView
	const handleOpenInCanvas = useCallback((filePath: string) => {
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
			if (!workspacePath) return;

			setIsUploading(true);
			const uploadedFiles: { name: string; path: string }[] = [];

			try {
				const baseUrl = fileserverWorkspaceBaseUrl();

				for (const file of Array.from(files)) {
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
				}

				setPendingUploads((prev) => [...prev, ...uploadedFiles]);
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
		[selectedChatFromHistory, selectedWorkspaceSession],
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
		if (messageInput.trim()) return false;
		if (pendingUploads.length > 0) return false;
		return !opencodeBaseUrl;
	}, [
		messageInput,
		opencodeBaseUrl,
		pendingUploads.length,
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
		// If we have this session in disk history but no live session, it's history-only
		if (
			selectedChatFromHistory &&
			selectedChatFromHistory.id === selectedChatSessionId
		) {
			return true;
		}
		return false;
	}, [selectedChatSession, selectedChatFromHistory, selectedChatSessionId]);

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
		isHistoryOnlySession,
		selectedChatFromHistory,
		selectedChatSessionId,
		selectedWorkspaceSessionId,
		setSelectedWorkspaceSessionId,
		workspaceSessions,
	]);

	// Merge messages to prevent flickering - preserves existing message references when unchanged
	const mergeMessages = useCallback(
		(
			prev: OpenCodeMessageWithParts[],
			next: OpenCodeMessageWithParts[],
		): OpenCodeMessageWithParts[] => {
			if (prev.length === 0) return next;
			if (next.length === 0) return next;

			// Build a map of existing messages by ID for quick lookup
			const prevById = new Map(prev.map((m) => [m.info.id, m]));

			// Merge: keep existing reference if message hasn't changed, otherwise use new one
			return next.map((newMsg) => {
				const existing = prevById.get(newMsg.info.id);
				if (!existing) return newMsg;

				// Compare parts length and last part to detect changes
				// This is a lightweight check to avoid deep comparison
				const existingParts = existing.parts;
				const newParts = newMsg.parts;

				if (existingParts.length !== newParts.length) return newMsg;

				// Check if the last part has changed (most common case during streaming)
				if (newParts.length > 0) {
					const lastNew = newParts[newParts.length - 1];
					const lastExisting = existingParts[existingParts.length - 1];

					// Compare text content or tool state
					if (lastNew.type === "text" && lastExisting.type === "text") {
						if (lastNew.text !== lastExisting.text) return newMsg;
					} else if (lastNew.type === "tool" && lastExisting.type === "tool") {
						if (
							lastNew.state?.status !== lastExisting.state?.status ||
							lastNew.state?.output !== lastExisting.state?.output
						) {
							return newMsg;
						}
					} else if (lastNew.type !== lastExisting.type) {
						return newMsg;
					}
				}

				// No significant changes detected, keep existing reference
				return existing;
			});
		},
		[],
	);

	// Load messages for Main Chat threaded view (all sessions combined)
	const loadMainChatThreadedMessages = useCallback(async () => {
		if (!mainChatAssistantName) return [];

		try {
			// Get all Main Chat sessions
			const sessions = await listMainChatSessions(mainChatAssistantName);
			if (sessions.length === 0) return [];

			// Sort sessions by date (oldest first for chronological thread)
			const sortedSessions = [...sessions].sort(
				(a, b) =>
					new Date(a.started_at).getTime() - new Date(b.started_at).getTime(),
			);

			// Load messages from each session and combine
			const allMessages: ThreadedMessage[] = [];

			for (const session of sortedSessions) {
				try {
					const historyMessages = await getChatMessages(session.session_id);
					if (historyMessages.length > 0) {
						const converted = convertChatMessagesToOpenCode(historyMessages);
						// Add session metadata to each message
						converted.forEach((msg, idx) => {
							const threadedMsg: ThreadedMessage = {
								...msg,
								_sessionId: session.session_id,
								_sessionTitle:
									session.title ||
									formatSessionDate(new Date(session.started_at).getTime()),
								_isSessionStart: idx === 0,
							};
							allMessages.push(threadedMsg);
						});
					}
				} catch {
					// Ignore failures for individual sessions
				}
			}

			return allMessages;
		} catch (err) {
			console.error("Failed to load Main Chat threaded messages:", err);
			return [];
		}
	}, [mainChatAssistantName]);

	const loadMessages = useCallback(async () => {
		// Main Chat threaded view - shows all sessions combined
		if (mainChatActive) {
			loadingSessionIdRef.current = "main-chat";
			if (mainChatAssistantName) {
				try {
					const threadedMessages = await loadMainChatThreadedMessages();
					// Check if we're still on main chat (session may have changed during async load)
					if (loadingSessionIdRef.current !== "main-chat") return;
					// Use merge to preserve optimistic messages (temp-* IDs)
					startTransition(() => {
						setMessages((prev) => {
							// Keep any optimistic messages (temp-* IDs) that aren't in the loaded messages
							const optimisticMessages = prev.filter((m) =>
								m.info.id.startsWith("temp-"),
							);
							if (optimisticMessages.length === 0) {
								return threadedMessages;
							}
							// Merge: loaded messages + optimistic messages at the end
							return [...threadedMessages, ...optimisticMessages];
						});
					});
				} catch (err) {
					setStatus((err as Error).message);
				}
			} else {
				setMessages([]);
			}
			return;
		}

		if (!selectedChatSessionId) return;

		// Capture session ID at start to detect stale responses
		const targetSessionId = selectedChatSessionId;
		loadingSessionIdRef.current = targetSessionId;

		try {
			let loadedMessages: OpenCodeMessageWithParts[] = [];

			if (opencodeBaseUrl && !isHistoryOnlySession) {
				// Live opencode is authoritative for streaming updates.
				loadedMessages = await fetchMessages(opencodeBaseUrl, targetSessionId, {
					directory: opencodeDirectory,
				});
			} else {
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

			if (
				loadedMessages.length === 0 &&
				opencodeBaseUrl &&
				!isHistoryOnlySession
			) {
				// If live returned nothing, fall back to disk history for older sessions.
				try {
					const historyMessages = await getChatMessages(targetSessionId);
					if (historyMessages.length > 0) {
						loadedMessages = convertChatMessagesToOpenCode(historyMessages);
					}
				} catch {
					// Ignore history failures on fallback.
				}
			}

			// Check if session changed during async load - discard stale response
			if (loadingSessionIdRef.current !== targetSessionId) {
				return;
			}

			// Use merge to prevent flickering when updating
			startTransition(() => {
				setMessages((prev) => mergeMessages(prev, loadedMessages));
			});
		} catch (err) {
			setStatus((err as Error).message);
		}
	}, [
		opencodeBaseUrl,
		opencodeDirectory,
		selectedChatSessionId,
		isHistoryOnlySession,
		mergeMessages,
		mainChatActive,
		mainChatAssistantName,
		loadMainChatThreadedMessages,
	]);

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
					await loadMessages();
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

	useEffect(() => {
		loadMessages();
	}, [loadMessages]);

	// Handle scroll events to show/hide scroll to bottom button
	const handleScroll = useCallback(() => {
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

		// Show button when not at bottom (use small threshold for better UX)
		setShowScrollToBottom(distanceFromBottom > 100);
	}, []);

	const messageCount = messages.length;

	// Check scroll position when messages change
	useEffect(() => {
		if (messageCount === 0) {
			setShowScrollToBottom(false);
		}
		handleScroll();
	}, [messageCount, handleScroll]);

	// Reset state when switching sessions
	useEffect(() => {
		if (!selectedChatSessionId) return;

		if (lastSessionIdRef.current !== selectedChatSessionId) {
			lastSessionIdRef.current = selectedChatSessionId;
			autoScrollEnabledRef.current = true;
			initialLoadRef.current = true;
		}
	}, [selectedChatSessionId]);

	// Position at bottom synchronously before paint (no visible jump)
	useLayoutEffect(() => {
		if (messages.length === 0) return;

		const container = messagesContainerRef.current;
		if (!container) return;

		if (initialLoadRef.current) {
			// Set scroll position directly - no animation, no visible jump
			container.scrollTop = container.scrollHeight;
			initialLoadRef.current = false;
		}
	}, [messages]);

	// Smooth scroll for new messages during conversation (after initial load)
	useEffect(() => {
		if (messages.length === 0) return;
		if (!autoScrollEnabledRef.current) return;
		if (initialLoadRef.current) return; // Skip - handled by useLayoutEffect

		scrollToBottom("smooth");
	}, [messages, scrollToBottom]);

	// Event handler for session events (shared between WebSocket and SSE)
	const handleSessionEvent = useCallback(
		(event: { type: string; properties?: Record<string, unknown> | null }) => {
			const eventType = event.type as string;

			// Debug: log all events to help diagnose permission issues
			if (eventType !== "message.updated") {
				console.log("[Event]", eventType, event.properties);
			}

			if (eventType === "transport.mode") {
				const props = event.properties as {
					mode?: "sse" | "polling" | "ws" | "reconnecting";
				} | null;
				if (props?.mode) setEventsTransportMode(props.mode);
				if (effectiveOpencodeBaseUrl && activeSessionId) {
					invalidateMessageCache(
						effectiveOpencodeBaseUrl,
						activeSessionId,
						opencodeDirectory,
					);
					requestMessageRefresh(250);
				}
				return;
			}

			if (eventType === "server.connected") {
				if (effectiveOpencodeBaseUrl && activeSessionId) {
					invalidateMessageCache(
						effectiveOpencodeBaseUrl,
						activeSessionId,
						opencodeDirectory,
					);
					requestMessageRefresh(250);
				}
			}

			if (eventType === "session.unavailable") {
				if (
					autoAttachMode === "resume" &&
					selectedChatFromHistory?.workspace_path
				) {
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
					void ensureOpencodeRunning(selectedChatFromHistory.workspace_path);
				}
			}

			if (eventType === "session.idle") {
				setChatState("idle");
				// Invalidate cache and force refresh on idle
				if (effectiveOpencodeBaseUrl && activeSessionId) {
					invalidateMessageCache(
						effectiveOpencodeBaseUrl,
						activeSessionId,
						opencodeDirectory,
					);
				}
				loadMessages();
				refreshOpencodeSessions();
				// Refresh chat history to pick up auto-generated session titles
				refreshChatHistory();
			} else if (eventType === "session.busy") {
				setChatState("sending");
			}

			// Handle permission events
			if (eventType === "permission.updated" || eventType === "permission.created") {
				const permission = normalizePermissionEvent(event.properties);
				if (!permission) return;
				console.log("[Permission] Received permission request:", permission);
				setPendingPermissions((prev) => {
					// Avoid duplicates
					if (prev.some((p) => p.id === permission.id)) return prev;
					return [...prev, permission];
				});
				// Auto-show the first permission dialog if none is active
				setActivePermission((current) => current || permission);
			} else if (
				eventType === "permission.replied" ||
				eventType === "permission.resolved"
			) {
				const props = event.properties as Record<string, unknown> | undefined;
				const permissionID =
					(typeof props?.permissionID === "string" && props.permissionID) ||
					(typeof props?.id === "string" && props.id) ||
					(typeof props?.permission_id === "string" && props.permission_id) ||
					"";
				if (!permissionID) return;
				console.log("[Permission] Permission replied:", permissionID);
				setPendingPermissions((prev) =>
					prev.filter((p) => p.id !== permissionID),
				);
				setActivePermission((current) =>
					current?.id === permissionID ? null : current,
				);
			}

			// Handle session errors
			if (eventType === "session.error") {
				const props = event.properties as Record<string, unknown> | undefined;
				const error =
					props && typeof props.error === "object" && props.error !== null
						? (props.error as {
								name?: string;
								data?: { message?: string };
							})
						: null;
				const errorName =
					(typeof props?.error_type === "string" && props.error_type) ||
					error?.name ||
					"Error";
				const errorMessage =
					(typeof props?.message === "string" && props.message) ||
					error?.data?.message ||
					"An unknown error occurred";
				console.error("[Session Error]", errorName, errorMessage);
				toast.error(errorMessage, {
					description: errorName !== "UnknownError" ? errorName : undefined,
					duration: 8000,
				});
				// Reset chat state on error
				setChatState("idle");
			}

			// Refresh messages on any message event
			if (eventType?.startsWith("message")) {
				// Invalidate cache when messages change
				if (effectiveOpencodeBaseUrl && activeSessionId) {
					invalidateMessageCache(
						effectiveOpencodeBaseUrl,
						activeSessionId,
						opencodeDirectory,
					);
				}
				// Coalesce refreshes to avoid hammering the server during streaming updates.
				requestMessageRefresh(1000);
			}
		},
		[
			autoAttachMode,
			ensureOpencodeRunning,
			effectiveOpencodeBaseUrl,
			opencodeDirectory,
			activeSessionId,
			selectedChatSessionId,
			selectedChatFromHistory,
			loadMessages,
			refreshOpencodeSessions,
			refreshChatHistory,
			requestMessageRefresh,
			setChatState,
		],
	);

	// Subscribe to session events (uses WebSocket when enabled, SSE otherwise)
	const { transportMode: sessionTransportMode } = useSessionEvents(
		handleSessionEvent,
		{
			useWebSocket: features.websocket_events ?? false,
			workspaceSessionId: selectedWorkspaceSessionId,
			opencodeBaseUrl: effectiveOpencodeBaseUrl,
			opencodeDirectory,
			activeSessionId,
			enabled: !!effectiveOpencodeBaseUrl && !!activeSessionId,
		},
	);

	// Sync transport mode from hook
	useEffect(() => {
		setEventsTransportMode(sessionTransportMode);
	}, [sessionTransportMode]);

	// Poll for message updates while assistant is working.
	// This runs regardless of SSE status since SSE is unreliable through the proxy.
	useEffect(() => {
		if (
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
		const startIndex = lastCompactionIndex >= 0 ? lastCompactionIndex + 1 : 0;

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
	}, [messages]);

	// Get context limit from models.dev based on current model
	const contextLimit = useModelContextLimit(
		tokenUsage.providerID,
		tokenUsage.modelID,
		200000, // Default fallback
	);

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

	// Extract the latest todo list from messages
	const latestTodos = useMemo(() => {
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

	// Handle slash command selection from popup
	const handleSlashCommandSelect = useCallback(
		async (cmd: SlashCommand) => {
			setShowSlashPopup(false);

			// Send opencode command (e.g., /init, /undo, /redo, or custom commands)
			if (!selectedChatSessionId || !opencodeBaseUrl) return;

			setMessageInput("");
			if (chatInputRef.current) {
				chatInputRef.current.style.height = "36px";
			}

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
					generateReadableId(sessionId);
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
		if (
			!messageInput.trim() &&
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

		// Capture file attachments before clearing
		const currentFileAttachments = [...fileAttachments];

		// Build message text with uploaded file paths
		let messageText = messageInput.trim();
		if (pendingUploads.length > 0) {
			const uploadPrefix =
				pendingUploads.length === 1
					? `[Uploaded file: ${pendingUploads[0].path}]`
					: `[Uploaded files: ${pendingUploads.map((u) => u.path).join(", ")}]`;
			messageText = messageText
				? `${uploadPrefix}\n\n${messageText}`
				: uploadPrefix;
		}

		// Check if this is a shell command (starts with "!")
		const isShellCommand = messageText.startsWith("!");
		const shellCommand = isShellCommand ? messageText.slice(1).trim() : "";

		setMessageInput("");
		// Reset textarea height to minimum
		if (chatInputRef.current) {
			chatInputRef.current.style.height = "36px";
		}
		setPendingUploads([]);
		setFileAttachments([]);
		setChatState("sending");
		setStatus("");

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
							generateReadableId(session.id) === resolvedMainChatSessionId,
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
						text: messageText,
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
					{ type: "text", text: messageText },
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
				await sendMessageAsync(
					effectiveBaseUrl,
					targetSessionId,
					messageText,
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
			loadMessages();
		} catch (err) {
			setStatus((err as Error).message);
			setChatState("idle");
			// Remove optimistic message on error
			setMessages((prev) => prev.filter((m) => !m.info.id.startsWith("temp-")));
		}
		// Don't set idle here - wait for SSE session.idle event
	};

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

			setStatus("");
		} catch (err) {
			setStatus((err as Error).message);
		}
	};

	const handleStop = async () => {
		if (!opencodeBaseUrl || !selectedChatSessionId) return;
		if (chatState !== "sending") return;

		try {
			await abortSession(opencodeBaseUrl, selectedChatSessionId, {
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
								<div className="text-sm text-muted-foreground">
									{t.configNotice}
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
	const readableId = selectedChatSession?.id
		? generateReadableId(selectedChatSession.id)
		: null;
	// Extract workspace name from path (last segment)
	const workspaceName = opencodeDirectory
		? opencodeDirectory.split("/").filter(Boolean).pop() || null
		: null;

	// Chat content component (reused in both layouts)
	const ChatContent = (
		<div
			ref={chatContainerRef}
			className="flex-1 flex flex-col gap-2 sm:gap-4 min-h-0"
		>
			{/* Permission banner */}
			<PermissionBanner
				count={pendingPermissions.length}
				onClick={handlePermissionBannerClick}
			/>

			{/* Working indicator with stop button */}
			{chatState === "sending" && (
				<div className="flex items-center gap-1.5 px-2 py-0.5 bg-primary/10 text-xs text-primary">
					<BrailleSpinner />
					<span className="font-medium flex-1">
						{locale === "de" ? "Agent arbeitet..." : "Agent working..."}
					</span>
					<button
						type="button"
						onClick={handleStop}
						className="mr-1 text-destructive hover:text-destructive/80 transition-colors"
						title={
							locale === "de" ? "Agent stoppen (2x Esc)" : "Stop agent (2x Esc)"
						}
					>
						<StopCircle className="w-5 h-5" />
					</button>
				</div>
			)}
			<div className="relative flex-1 min-h-0">
				<div
					ref={messagesContainerRef}
					onScroll={handleScroll}
					className="h-full bg-muted/30 border border-border p-2 sm:p-4 overflow-y-auto space-y-4 sm:space-y-6 scrollbar-hide"
				>
					{messages.length === 0 && (
						<div className="text-sm text-muted-foreground">{t.noMessages}</div>
					)}
					{hasHiddenMessages && (
						<button
							type="button"
							onClick={loadMoreMessages}
							className="w-full py-2 text-xs text-muted-foreground hover:text-foreground hover:bg-muted/50 border border-dashed border-border transition-colors"
						>
							{locale === "de"
								? `${messageGroups.length - visibleGroupCount} altere Nachrichten laden...`
								: `Load ${messageGroups.length - visibleGroupCount} older messages...`}
						</button>
					)}
					{visibleGroups.map((group) => (
						<div
							key={
								group.messages[0]?.info.id ||
								`${group.role}-${group.startIndex}`
							}
						>
							{/* Session divider for Main Chat threaded view */}
							{group.isNewSession && group.sessionTitle && (
								<SessionDivider title={group.sessionTitle} />
							)}
							<MessageGroupCard
								group={group}
								persona={selectedSession?.persona}
								workspaceName={workspaceName}
								readableId={readableId}
								workspaceDirectory={opencodeDirectory}
								onFork={handleForkSession}
								locale={locale}
							/>
						</div>
					))}
					<div ref={messagesEndRef} />
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
				{/* Show hint for history sessions that will be resumed */}
				{isHistoryOnlySession && (
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
						className="flex-shrink-0 size-8 flex items-center justify-center text-muted-foreground hover:text-foreground disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
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
					<div className="flex-1 relative flex flex-col min-h-[32px]">
						<SlashCommandPopup
							commands={slashCommands}
							query={slashQuery.command}
							isOpen={showSlashPopup && slashQuery.isSlash && !slashQuery.args}
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
								setMessageInput(newInput);
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
						<textarea
							ref={chatInputRef}
							placeholder={
								dictation.isActive && dictation.liveTranscript
									? dictation.liveTranscript
									: isHistoryOnlySession
										? locale === "de"
											? "Nachricht zum Fortsetzen..."
											: "Message to resume..."
										: dictation.isActive
											? locale === "de"
												? "Sprechen Sie..."
												: "Speak now..."
											: t.inputPlaceholder
							}
							value={messageInput}
							onChange={(e) => {
								const value = e.target.value;
								setMessageInput(value);
								// Show slash popup when typing /
								if (value.startsWith("/")) {
									setShowSlashPopup(true);
									setShowFileMentionPopup(false);
								} else {
									setShowSlashPopup(false);
								}
								// Show file mention popup when typing @
								const atMatch = value.match(/@([^\s]*)$/);
								if (atMatch && !value.startsWith("/")) {
									setShowFileMentionPopup(true);
									setFileMentionQuery(atMatch[1]);
								} else {
									setShowFileMentionPopup(false);
									setFileMentionQuery("");
								}
								// Auto-resize is handled by useEffect on messageInput change
							}}
							onKeyDown={(e) => {
								// Let slash popup handle arrow keys, enter, tab when open
								if (showSlashPopup && slashQuery.isSlash && !slashQuery.args) {
									if (
										["ArrowDown", "ArrowUp", "Enter", "Tab", "Escape"].includes(
											e.key,
										)
									) {
										// Popup will handle these via its own event listener
										return;
									}
								}
								// Let file mention popup handle its keys
								if (showFileMentionPopup) {
									if (
										["ArrowDown", "ArrowUp", "Enter", "Tab", "Escape"].includes(
											e.key,
										)
									) {
										// Popup handles via its own event listener
										return;
									}
								}
								if (e.key === "Enter" && !e.shiftKey) {
									e.preventDefault();
									handleSend();
									// Reset textarea height after sending
									if (chatInputRef.current) {
										chatInputRef.current.style.height = "auto";
									}
								}
								if (e.key === "Escape") {
									setShowSlashPopup(false);
									setShowFileMentionPopup(false);
								}
							}}
							onPaste={(e) => {
								// Handle pasted files (images, etc.)
								const items = e.clipboardData?.items;
								if (!items) return;

								const files: File[] = [];
								for (const item of Array.from(items)) {
									if (item.kind === "file") {
										const file = item.getAsFile();
										if (file) {
											files.push(file);
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
								if (messageInput.startsWith("/")) {
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
					</div>
					<Button
						type="button"
						data-voice-send
						onClick={canResumeWithoutMessage ? handleResume : handleSend}
						disabled={
							!canResumeWithoutMessage &&
							!messageInput.trim() &&
							pendingUploads.length === 0 &&
							fileAttachments.length === 0
						}
						className="bg-primary hover:bg-primary/90 text-primary-foreground"
					>
						{canResumeWithoutMessage ? (
							<RefreshCw className="w-4 h-4 sm:mr-2" />
						) : (
							<Send className="w-4 h-4 sm:mr-2" />
						)}
						<span className="hidden sm:inline">
							{canResumeWithoutMessage
								? locale === "de"
									? "Fortsetzen"
									: "Resume"
								: t.send}
						</span>
					</Button>
				</div>
			</div>
		</div>
	);

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
			setContinuous: voiceMode.setContinuous,
			setVoice: voiceMode.setVoice,
			setSpeed: voiceMode.setSpeed,
			setVadTimeout: voiceMode.setVadTimeout,
			setInterruptWordCount: voiceMode.setInterruptWordCount,
		},
	};

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
	const showModelSwitcher =
		!mainChatActive && !!effectiveOpencodeBaseUrl && !!activeSessionId;
	const filteredModelOptions = filterModelOptions(
		opencodeModelOptions,
		modelQuery,
	);
	const modelSwitcher = showModelSwitcher ? (
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
						No models available
					</SelectItem>
				) : filteredModelOptions.length === 0 ? (
					<SelectItem value="__no_results__" disabled>
						No matches
					</SelectItem>
				) : (
					filteredModelOptions.map((option) => (
						<SelectItem key={option.value} value={option.value}>
							{option.label}
						</SelectItem>
					))
				)}
			</SelectContent>
		</Select>
	) : null;
	const persona = selectedSession?.persona;
	const SessionHeader = (
		<div className="flex items-center justify-between pb-3 mb-3 border-b border-border">
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
							<span className="font-mono">
								{workspaceName}
								{readableId && ` [${readableId}]`}
							</span>
						)}
						{(workspaceName || readableId) && formattedDate && (
							<span className="opacity-50">|</span>
						)}
						{formattedDate && <span>{formattedDate}</span>}
					</div>
				</div>
			</div>
			<div className="flex items-center gap-3 flex-shrink-0 ml-2">
				{status && <span className="text-xs text-destructive">{status}</span>}
				{modelSwitcher}
				<ContextWindowGauge
					inputTokens={tokenUsage.inputTokens}
					outputTokens={tokenUsage.outputTokens}
					maxTokens={contextLimit}
					locale={locale}
				/>
			</div>
		</div>
	);

	return (
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
							view="preview"
							icon={Eye}
							label={t.preview}
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
					{!mainChatActive && (
						<ContextWindowGauge
							inputTokens={tokenUsage.inputTokens}
							outputTokens={tokenUsage.outputTokens}
							maxTokens={contextLimit}
							locale={locale}
							compact
						/>
					)}
				</div>

				{/* Mobile content */}
				<div className="flex-1 min-h-0 bg-card border border-t-0 border-border rounded-b-xl p-1.5 sm:p-4 overflow-hidden flex flex-col">
					{activeView === "chat" &&
						(mainChatActive ? (
							<MainChatPiView
								locale={locale}
								className="flex-1"
								features={features}
								workspacePath={mainChatWorkspacePath}
								assistantName={mainChatAssistantName}
							/>
						) : (
							ChatContent
						))}
					{activeView === "files" && (
						<FileTreeView
							onPreviewFile={handlePreviewFile}
							onOpenInCanvas={handleOpenInCanvas}
							workspacePath={resumeWorkspacePath}
							state={fileTreeState}
							onStateChange={handleFileTreeStateChange}
						/>
					)}
					{activeView === "preview" && (
						<Suspense fallback={viewLoadingFallback}>
							<PreviewView
								filePath={previewFilePath}
								workspacePath={resumeWorkspacePath}
							/>
						</Suspense>
					)}
					{activeView === "tasks" && (
						<div className="flex flex-col h-full overflow-hidden">
							<TodoListView todos={latestTodos} emptyMessage={t.noTasks} />
							<Suspense fallback={viewLoadingFallback}>
								<TrxView
									workspacePath={resumeWorkspacePath}
									className="flex-1 min-h-0 border-t border-border"
								/>
							</Suspense>
						</div>
					)}
					{features.mmry_enabled && activeView === "memories" && (
						<Suspense fallback={viewLoadingFallback}>
							<MemoriesView
								workspacePath={resumeWorkspacePath}
								storeName={mainChatActive ? mainChatAssistantName : null}
							/>
						</Suspense>
					)}
					{activeView === "settings" && (
						<Suspense fallback={viewLoadingFallback}>
							<AgentSettingsView />
						</Suspense>
					)}
					{activeView === "canvas" && (
						<Suspense fallback={viewLoadingFallback}>
							<CanvasView
								workspacePath={resumeWorkspacePath}
								initialImagePath={previewFilePath}
								onSaveAndAddToChat={handleCanvasSaveAndAddToChat}
							/>
						</Suspense>
					)}
					{/* Terminal only rendered in mobile layout when isMobileLayout is true */}
					{isMobileLayout && (
						<div className={activeView === "terminal" ? "h-full" : "hidden"}>
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
				<div className="flex-[3] min-w-0 bg-card border border-border p-4 xl:p-6 flex flex-col min-h-0 h-full">
					{!mainChatActive && SessionHeader}
					{mainChatActive ? (
						<MainChatPiView
							locale={locale}
							className="flex-1"
							features={features}
							workspacePath={mainChatWorkspacePath}
							assistantName={mainChatAssistantName}
						/>
					) : (
						ChatContent
					)}
				</div>

				{/* Sidebar panel */}
				<div className="flex-[2] min-w-[320px] max-w-[420px] bg-card border border-border flex flex-col min-h-0 h-full">
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
							view="preview"
							icon={Eye}
							label={t.preview}
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
						{activeView === "files" && (
							<FileTreeView
								onPreviewFile={handlePreviewFile}
								onOpenInCanvas={handleOpenInCanvas}
								workspacePath={resumeWorkspacePath}
								state={fileTreeState}
								onStateChange={handleFileTreeStateChange}
							/>
						)}
						{activeView === "preview" && (
							<Suspense fallback={viewLoadingFallback}>
								<PreviewView
									filePath={previewFilePath}
									workspacePath={resumeWorkspacePath}
								/>
							</Suspense>
						)}
						{activeView === "tasks" && (
							<div className="flex flex-col h-full overflow-hidden">
								<TodoListView todos={latestTodos} emptyMessage={t.noTasks} />
								<Suspense fallback={viewLoadingFallback}>
									<TrxView
										workspacePath={resumeWorkspacePath}
										className="flex-1 min-h-0 border-t border-border"
									/>
								</Suspense>
							</div>
						)}
						{activeView === "chat" && (
							<TodoListView todos={latestTodos} emptyMessage={t.noTasks} />
						)}
						{features.mmry_enabled && activeView === "memories" && (
							<Suspense fallback={viewLoadingFallback}>
								<MemoriesView
									workspacePath={resumeWorkspacePath}
									storeName={mainChatActive ? mainChatAssistantName : null}
								/>
							</Suspense>
						)}
						{activeView === "voice" && voiceMode.isActive && (
							<VoicePanel {...voicePanelProps} />
						)}
						{activeView === "settings" && (
							<Suspense fallback={viewLoadingFallback}>
								<AgentSettingsView />
							</Suspense>
						)}
						{activeView === "canvas" && (
							<Suspense fallback={viewLoadingFallback}>
								<CanvasView
									workspacePath={resumeWorkspacePath}
									initialImagePath={previewFilePath}
									onSaveAndAddToChat={handleCanvasSaveAndAddToChat}
								/>
							</Suspense>
						)}
						{/* Terminal only rendered in desktop layout when isMobileLayout is false */}
						{!isMobileLayout && (
							<div className={activeView === "terminal" ? "h-full" : "hidden"}>
								<Suspense fallback={viewLoadingFallback}>
									<TerminalView workspacePath={resumeWorkspacePath} />
								</Suspense>
							</div>
						)}
					</div>
				</div>
			</div>

			{/* Permission dialog */}
			<PermissionDialog
				permission={activePermission}
				onRespond={handlePermissionResponse}
				onDismiss={handlePermissionDismiss}
			/>

			{/* Voice mode overlay - mobile only */}
			{mobileVoiceOverlay}
		</div>
	);
}

const MessageGroupCard = memo(function MessageGroupCard({
	group,
	persona,
	workspaceName,
	readableId,
	workspaceDirectory,
	onFork,
	locale = "en",
}: {
	group: MessageGroup;
	persona?: Persona | null;
	workspaceName?: string | null;
	readableId?: string | null;
	workspaceDirectory?: string;
	onFork?: (messageId: string) => void;
	locale?: "de" | "en";
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

	// Group consecutive text parts together, but keep tool and other parts separate
	// This creates "segments" that maintain the original order
	type Segment =
		| { key: string; type: "text"; content: string }
		| { key: string; type: "tool"; part: OpenCodePart }
		| { key: string; type: "file"; part: OpenCodePart }
		| { key: string; type: "other"; part: OpenCodePart };

	const segments: Segment[] = [];
	let currentTextBuffer: string[] = [];
	let currentTextKeys: string[] = [];

	const flushTextBuffer = () => {
		if (currentTextBuffer.length > 0) {
			const key = currentTextKeys[0] ?? `text-${segments.length}`;
			segments.push({
				key,
				type: "text",
				content: currentTextBuffer.join("\n\n"),
			});
			currentTextBuffer = [];
			currentTextKeys = [];
		}
	};

	for (const [index, part] of allParts.entries()) {
		const partKey = part.id ?? `${part.type}-${index}`;
		if (part.type === "text" && typeof part.text === "string") {
			currentTextBuffer.push(part.text);
			currentTextKeys.push(partKey);
		} else if (part.type === "tool") {
			flushTextBuffer();
			segments.push({ key: partKey, type: "tool", part });
		} else if (part.type === "file") {
			flushTextBuffer();
			segments.push({ key: partKey, type: "file", part });
		} else {
			flushTextBuffer();
			segments.push({ key: partKey, type: "other", part });
		}
	}
	flushTextBuffer();

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
				{segments.length === 0 && !isUser && (
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

				{segments.map((segment) => {
					if (segment.type === "text") {
						// Parse @file references from the text
						const fileRefPattern = /@([^\s@]+\.[a-zA-Z0-9]+)/g;
						const matches = segment.content.match(fileRefPattern) || [];
						const fileRefs = matches.map((m) => m.slice(1)); // Remove @ prefix
						// Remove duplicates
						const uniqueFileRefs = [...new Set(fileRefs)];

						return (
							<div key={segment.key} className="overflow-hidden space-y-2">
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
						);
					}

					if (segment.type === "file") {
						return (
							<FilePartCard
								key={segment.key}
								part={segment.part}
								workspaceDirectory={workspaceDirectory}
							/>
						);
					}

					if (segment.type === "tool") {
						return (
							<ToolCallCard
								key={segment.key}
								part={segment.part}
								defaultCollapsed={true}
								hideTodoTools={true}
							/>
						);
					}

					if (segment.type === "other") {
						return <OtherPartCard key={segment.key} part={segment.part} />;
					}

					return null;
				})}
			</div>
		</div>
	);

	// If no fork handler, just render the card directly
	if (!onFork || !lastMessageId) {
		return messageCard;
	}

	// Wrap in context menu for fork functionality
	return (
		<ContextMenu>
			<ContextMenuTrigger asChild>{messageCard}</ContextMenuTrigger>
			<ContextMenuContent>
				<ContextMenuItem
					onClick={() => onFork(lastMessageId)}
					className="gap-2"
				>
					<GitBranch className="w-4 h-4" />
					{locale === "de" ? "Von hier verzweigen" : "Branch from here"}
				</ContextMenuItem>
				<ContextMenuSeparator />
				<ContextMenuItem
					onClick={() => {
						if (allTextContent) {
							navigator.clipboard?.writeText(allTextContent);
						}
					}}
					className="gap-2"
				>
					<Copy className="w-4 h-4" />
					{locale === "de" ? "Text kopieren" : "Copy text"}
				</ContextMenuItem>
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
			return { filePath: metadataPath || fileName, fileName, directUrl: rawUrl };
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

/** Renders a file reference card with preview for images */
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

	const fileInfo = useMemo(() => getFileTypeInfo(filePath), [filePath]);
	const isImage = fileInfo.category === "image";
	const fileName = label || filePath.split("/").pop() || filePath;

	// Build the file URL
	const fileUrl = useMemo(() => {
		if (directUrl) return directUrl;
		if (!workspacePath) return null;
		const baseUrl = fileserverWorkspaceBaseUrl();
		const encodedPath = encodeURIComponent(filePath);
		const workspaceParam = `&workspace_path=${encodeURIComponent(workspacePath)}`;
		return `${baseUrl}/read?path=${encodedPath}${workspaceParam}`;
	}, [directUrl, filePath, workspacePath]);

	if (!fileUrl) {
		return (
			<div className="inline-flex items-center gap-2 px-3 py-1.5 border border-border bg-muted/20 rounded text-xs text-muted-foreground">
				<FileText className="w-4 h-4" />
				<span className="truncate max-w-[220px]">{fileName}</span>
			</div>
		);
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

	// For non-images, render a compact file reference link
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
}: { todos: TodoItem[]; emptyMessage: string }) {
	// Group todos by status for summary
	const summary = useMemo(() => {
		const pending = todos.filter((t) => t.status === "pending").length;
		const inProgress = todos.filter((t) => t.status === "in_progress").length;
		const completed = todos.filter((t) => t.status === "completed").length;
		const cancelled = todos.filter((t) => t.status === "cancelled").length;
		return { pending, inProgress, completed, cancelled, total: todos.length };
	}, [todos]);

	if (todos.length === 0) {
		return (
			<div className="flex items-center justify-center h-full p-4">
				<div className="text-center">
					<ListTodo className="w-12 h-12 text-muted-foreground/30 mx-auto mb-3" />
					<p className="text-sm text-muted-foreground">{emptyMessage}</p>
				</div>
			</div>
		);
	}

	return (
		<div className="flex flex-col h-full">
			{/* Summary header */}
			<div className="p-3 border-b border-border bg-muted/30">
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
			<div className="flex-1 overflow-y-auto p-2 space-y-1">
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

export default SessionsApp;
