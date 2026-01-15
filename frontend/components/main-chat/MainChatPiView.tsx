"use client";

import { A2UICallCard } from "@/components/ui/a2ui-call-card";
import { BrailleSpinner } from "@/components/ui/braille-spinner";
import { Button } from "@/components/ui/button";
import {
	ContextMenu,
	ContextMenuContent,
	ContextMenuItem,
	ContextMenuTrigger,
} from "@/components/ui/context-menu";
import { ContextWindowGauge } from "@/components/ui/context-window-gauge";
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
import { ReadAloudButton } from "@/components/ui/read-aloud-button";
import { SlashCommandPopup } from "@/components/ui/slash-command-popup";
import { ToolCallCard } from "@/components/ui/tool-call-card";
import { DictationOverlay } from "@/components/voice";
import {
	VoiceMenuButton,
	type VoiceMode,
} from "@/components/voice/VoiceMenuButton";
import { type A2UISurfaceState, useA2UI } from "@/hooks/use-a2ui";
import { useDictation } from "@/hooks/use-dictation";
import {
	type PiDisplayMessage,
	type PiMessagePart,
	getCachedScrollPosition,
	setCachedScrollPosition,
	usePiChat,
} from "@/hooks/usePiChat";
import {
	type Features,
	type PiModelInfo,
	compactMainChatPi,
	fileserverWorkspaceBaseUrl,
	getMainChatPiCommands,
	getMainChatPiModels,
	getMainChatPiStats,
	setMainChatPiModel,
	workspaceFileUrl,
} from "@/lib/control-plane-client";
import { extractFileReferences, getFileTypeInfo } from "@/lib/file-types";
import {
	type SlashCommand,
	fuzzyMatch,
	parseSlashInput,
} from "@/lib/slash-commands";
import { cn } from "@/lib/utils";
import {
	Bot,
	Copy,
	ExternalLink,
	File,
	FileVideo,
	ImageIcon,
	Loader2,
	Paperclip,
	Send,
	StopCircle,
	User,
} from "lucide-react";
import {
	type ChangeEvent,
	type KeyboardEvent,
	memo,
	useCallback,
	useEffect,
	useLayoutEffect,
	useMemo,
	useRef,
	useState,
} from "react";

export interface MainChatPiViewProps {
	/** Current locale */
	locale?: "en" | "de";
	/** Class name for container */
	className?: string;
	/** Features config (for voice settings) */
	features?: Features | null;
	/** Workspace path for file operations */
	workspacePath?: string | null;
	/** Assistant name to display (user-configured main chat name) */
	assistantName?: string | null;
	/** Hide the internal header (used when embedded in sessions app with external header) */
	hideHeader?: boolean;
	/** Callback to report token usage (for external gauge display) */
	onTokenUsageChange?: (usage: {
		inputTokens: number;
		outputTokens: number;
		maxTokens: number;
	}) => void;
	/** Session ID to scroll to (when clicking session in sidebar) */
	scrollToSessionId?: string | null;
	/** Message ID to scroll to (from search results) */
	scrollToMessageId?: string | null;
	/** Callback when scroll target is reached (to clear the target) */
	onScrollToMessageComplete?: () => void;
}

/**
 * Main Chat view using Pi agent runtime.
 * Styled to match OpenCode chat UI exactly.
 */
export function MainChatPiView({
	locale = "en",
	className,
	features,
	workspacePath,
	assistantName,
	hideHeader = false,
	onTokenUsageChange,
	scrollToSessionId,
	scrollToMessageId,
	onScrollToMessageComplete,
}: MainChatPiViewProps) {
	const {
		messages,
		isConnected,
		isStreaming,
		error,
		send,
		abort,
		newSession,
		resetSession,
		state: piState,
		refresh,
	} = usePiChat();

	// Draft persistence - restore from localStorage on mount
	const [input, setInput] = useState(() => {
		if (typeof window === "undefined") return "";
		try {
			return localStorage.getItem("octo:mainChatDraft") || "";
		} catch {
			return "";
		}
	});
	const [fileAttachments, setFileAttachments] = useState<FileAttachment[]>([]);
	const [showFileMentionPopup, setShowFileMentionPopup] = useState(false);
	const [fileMentionQuery, setFileMentionQuery] = useState("");
	const [showSlashPopup, setShowSlashPopup] = useState(false);
	const [voiceMode, setVoiceMode] = useState<VoiceMode>(null);
	const [isUploading, setIsUploading] = useState(false);
	const [availableModels, setAvailableModels] = useState<PiModelInfo[]>([]);
	const [selectedModelRef, setSelectedModelRef] = useState<string | null>(null);
	const [isSwitchingModel, setIsSwitchingModel] = useState(false);
	const [modelQuery, setModelQuery] = useState("");
	const [commandError, setCommandError] = useState<Error | null>(null);
	const [customCommands, setCustomCommands] = useState<SlashCommand[]>([]);
	const [sessionTokens, setSessionTokens] = useState<{
		input: number;
		output: number;
	} | null>(null);

	const messagesEndRef = useRef<HTMLDivElement>(null);
	const messagesContainerRef = useRef<HTMLDivElement>(null);
	const inputRef = useRef<HTMLTextAreaElement>(null);
	const fileInputRef = useRef<HTMLInputElement>(null);
	const draftSaveTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(
		null,
	);
	const initialScrollDoneRef = useRef(false);
	// Initialize from cached scroll position - null means bottom
	const [isUserScrolled, setIsUserScrolled] = useState(
		() => getCachedScrollPosition() !== null,
	);
	// Pagination: start with last 30 messages, load more on scroll up
	const INITIAL_MESSAGES = 30;
	const LOAD_MORE_COUNT = 30;
	const [visibleCount, setVisibleCount] = useState(INITIAL_MESSAGES);

	// A2UI integration - adapt Pi messages to expected format
	const a2uiMessagesRef = useRef<Array<{ info: { id: string; role: string } }>>(
		[],
	);
	// Keep ref in sync with messages
	a2uiMessagesRef.current = messages.map((m) => ({
		info: { id: m.id, role: m.role },
	}));

	const { surfaces: a2uiSurfaces, handleAction: handleA2UIAction, getUnanchoredSurfaces } = useA2UI(
		a2uiMessagesRef,
		{
			onSurfaceReceived: useCallback(() => {
				// Auto-scroll when A2UI surface arrives
				if (!isUserScrolled) {
					messagesEndRef.current?.scrollIntoView({ behavior: "smooth" });
				}
			}, [isUserScrolled]),
		},
	);

	// Memoized map of message ID to surfaces to avoid creating new arrays on each render
	// Also track "orphaned" surfaces whose anchor doesn't match any current message
	const { surfacesByMessageId, orphanedSurfaces } = useMemo(() => {
		const map = new Map<string, A2UISurfaceState[]>();
		const messageIds = new Set(messages.map((m) => m.id));
		const orphaned: A2UISurfaceState[] = [];
		
		for (const surface of a2uiSurfaces) {
			if (surface.anchorMessageId && messageIds.has(surface.anchorMessageId)) {
				const existing = map.get(surface.anchorMessageId) || [];
				existing.push(surface);
				map.set(surface.anchorMessageId, existing);
			} else {
				// Surface has no anchor or anchor message doesn't exist
				orphaned.push(surface);
			}
		}
		return { surfacesByMessageId: map, orphanedSurfaces: orphaned };
	}, [a2uiSurfaces, messages]);

	// Voice configuration
	const voiceConfig = useMemo(
		() =>
			features?.voice
				? {
						stt_url: features.voice.stt_url,
						tts_url: features.voice.tts_url,
						vad_timeout_ms: features.voice.vad_timeout_ms,
						default_voice: features.voice.default_voice,
						default_speed: features.voice.default_speed,
					}
				: null,
		[features?.voice],
	);

	const currentModelRef = useMemo(() => {
		if (!piState?.model) return null;
		return `${piState.model.provider}/${piState.model.id}`;
	}, [piState?.model]);
	const currentModelInfo = useMemo(() => {
		if (piState?.model) {
			return piState.model;
		}
		if (!selectedModelRef) return null;
		return (
			availableModels.find(
				(model) => `${model.provider}/${model.id}` === selectedModelRef,
			) ?? null
		);
	}, [availableModels, piState?.model, selectedModelRef]);
	const contextWindowLimit = useMemo(() => {
		if (!currentModelInfo) return 200000;
		if (currentModelInfo.context_window > 0) {
			return currentModelInfo.context_window;
		}
		if (currentModelInfo.max_tokens > 0) {
			return currentModelInfo.max_tokens;
		}
		return 200000;
	}, [currentModelInfo]);
	const slashQuery = useMemo(() => parseSlashInput(input), [input]);
	const builtInCommands = useMemo<SlashCommand[]>(
		() => [
			{ name: "compact", description: "Summarize context" },
			{ name: "new", description: "Start a fresh session" },
			{ name: "reset", description: "Reload personality and user files" },
			{ name: "abort", description: "Abort current run" },
			{ name: "steer", description: "Queue a steering message" },
			{ name: "followup", description: "Queue a follow-up message" },
			{ name: "model", description: "Switch model (provider/model)" },
		],
		[],
	);
	const builtInCommandNames = useMemo(
		() => new Set(builtInCommands.map((cmd) => cmd.name)),
		[builtInCommands],
	);
	const slashCommands = useMemo<SlashCommand[]>(() => {
		const merged = [...builtInCommands];
		const seen = new Set(merged.map((cmd) => cmd.name));
		for (const cmd of customCommands) {
			if (!seen.has(cmd.name)) {
				merged.push(cmd);
				seen.add(cmd.name);
			}
		}
		return merged;
	}, [builtInCommands, customCommands]);
	const displayError = commandError ?? error;
	const filteredModels = useMemo(() => {
		const query = modelQuery.trim();
		if (!query) return availableModels;
		return availableModels.filter((model) => {
			const fullRef = `${model.provider}/${model.id}`;
			return (
				fuzzyMatch(query, fullRef) ||
				fuzzyMatch(query, model.provider) ||
				fuzzyMatch(query, model.id) ||
				(model.name ? fuzzyMatch(query, model.name) : false)
			);
		});
	}, [availableModels, modelQuery]);
	const messageTokenUsage = useMemo(() => {
		let inputTokens = 0;
		let outputTokens = 0;

		// Find the index of the last compaction message
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
			if (!msg.usage) continue;
			inputTokens += msg.usage.input || 0;
			outputTokens += msg.usage.output || 0;
		}
		return { inputTokens, outputTokens };
	}, [messages]);
	const gaugeTokens = useMemo(() => {
		const total =
			messageTokenUsage.inputTokens + messageTokenUsage.outputTokens;
		if (total > 0) return messageTokenUsage;
		if (sessionTokens) {
			return {
				inputTokens: sessionTokens.input,
				outputTokens: sessionTokens.output,
			};
		}
		return { inputTokens: 0, outputTokens: 0 };
	}, [messageTokenUsage, sessionTokens]);

	// Report token usage to parent when it changes
	useEffect(() => {
		if (onTokenUsageChange) {
			onTokenUsageChange({
				inputTokens: gaugeTokens.inputTokens,
				outputTokens: gaugeTokens.outputTokens,
				maxTokens: contextWindowLimit,
			});
		}
	}, [gaugeTokens, contextWindowLimit, onTokenUsageChange]);

	// Dictation hook
	const dictation = useDictation({
		config: voiceConfig,
		onTranscript: useCallback((text: string) => {
			setInput((prev) => (prev ? `${prev} ${text}` : text));
		}, []),
		vadTimeoutMs: features?.voice?.vad_timeout_ms,
		autoSendOnFinal: true,
		autoSendDelayMs: 50,
		onAutoSend: () => {
			const sendBtn = document.querySelector(
				"[data-dictation-send]",
			) as HTMLButtonElement | null;
			sendBtn?.click();
		},
	});

	useEffect(() => {
		if (selectedModelRef || !currentModelRef) return;
		setSelectedModelRef(currentModelRef);
	}, [currentModelRef, selectedModelRef]);

	useEffect(() => {
		if (!isConnected) return;
		let active = true;
		getMainChatPiModels()
			.then((models) => {
				if (active) setAvailableModels(models);
			})
			.catch(() => {
				if (active) setAvailableModels([]);
			});
		return () => {
			active = false;
		};
	}, [isConnected]);

	useEffect(() => {
		if (!isConnected) return;
		let active = true;
		getMainChatPiCommands()
			.then((commands) => {
				if (!active) return;
				setCustomCommands(
					commands.map((cmd) => ({
						name: cmd.name,
						description: cmd.description,
					})),
				);
			})
			.catch(() => {
				if (active) setCustomCommands([]);
			});
		return () => {
			active = false;
		};
	}, [isConnected]);

	const refreshStats = useCallback(async () => {
		try {
			const stats = await getMainChatPiStats();
			if (stats.tokens) {
				setSessionTokens({
					input: stats.tokens.input ?? 0,
					output: stats.tokens.output ?? 0,
				});
			}
		} catch {
			// Ignore stats errors; token gauge will fall back to message usage.
		}
	}, []);

	// biome-ignore lint/correctness/useExhaustiveDependencies: messages.length triggers refresh when message count changes
	useEffect(() => {
		if (!isConnected || isStreaming) return;
		refreshStats();
	}, [isConnected, isStreaming, messages.length, refreshStats]);

	// Scroll to session when scrollToSessionId changes
	useEffect(() => {
		if (!scrollToSessionId || !messagesContainerRef.current) return;

		// Find the separator element with this session ID
		const separator = messagesContainerRef.current.querySelector(
			`[data-session-id="${scrollToSessionId}"]`,
		);

		if (separator) {
			separator.scrollIntoView({ behavior: "smooth", block: "start" });
			setIsUserScrolled(true); // Prevent auto-scroll from overriding
		}
	}, [scrollToSessionId]);

	// Scroll to message when scrollToMessageId changes (from search results)
	useEffect(() => {
		if (!scrollToMessageId || !messagesContainerRef.current) return;

		// Find the message element with this ID
		const messageEl = messagesContainerRef.current.querySelector(
			`[data-message-id="${scrollToMessageId}"]`,
		);

		if (messageEl) {
			// Ensure we have enough messages visible
			const messageIndex = messages.findIndex(
				(m) => m.id === scrollToMessageId,
			);
			if (messageIndex !== -1) {
				const messagesFromEnd = messages.length - messageIndex;
				if (messagesFromEnd > visibleCount) {
					setVisibleCount(messagesFromEnd + 10);
				}
			}

			// Scroll to the message
			requestAnimationFrame(() => {
				messageEl.scrollIntoView({ behavior: "smooth", block: "center" });
				// Add highlight animation
				messageEl.classList.add("search-highlight");
				setTimeout(() => {
					messageEl.classList.remove("search-highlight");
				}, 2000);
			});

			setIsUserScrolled(true);
			onScrollToMessageComplete?.();
		}
	}, [scrollToMessageId, messages, visibleCount, onScrollToMessageComplete]);

	// Initial scroll position - instant, no animation
	useLayoutEffect(() => {
		if (initialScrollDoneRef.current || !messagesContainerRef.current) return;
		if (messages.length === 0) return;

		initialScrollDoneRef.current = true;
		const container = messagesContainerRef.current;
		const cachedPosition = getCachedScrollPosition();

		if (cachedPosition !== null) {
			// Restore user's scroll position instantly
			container.scrollTop = cachedPosition;
		} else {
			// Scroll to bottom instantly (no animation)
			container.scrollTop = container.scrollHeight;
		}
	}, [messages.length]);

	// Auto-scroll to bottom when NEW messages arrive (only if user hasn't scrolled up)
	const prevMessageCountRef = useRef(messages.length);
	useLayoutEffect(() => {
		const prevCount = prevMessageCountRef.current;
		prevMessageCountRef.current = messages.length;

		// Only auto-scroll if messages were added (not on initial load)
		if (messages.length > prevCount) {
			// Ensure new messages are visible
			setVisibleCount((prev) => Math.max(prev, INITIAL_MESSAGES));
			if (!isUserScrolled) {
				messagesEndRef.current?.scrollIntoView({ behavior: "smooth" });
			}
		}
	}, [messages.length, isUserScrolled]);

	// Detect user scroll to disable auto-scroll, save position, and load more messages
	const handleScroll = useCallback(() => {
		const container = messagesContainerRef.current;
		if (!container) return;

		// Check if user is near the bottom (within 100px)
		const isNearBottom =
			container.scrollHeight - container.scrollTop - container.clientHeight <
			100;

		// Check if user scrolled to the top - load more messages
		const isNearTop = container.scrollTop < 100;
		if (isNearTop && visibleCount < messages.length) {
			// Preserve scroll position when loading more
			const prevScrollHeight = container.scrollHeight;
			setVisibleCount((prev) =>
				Math.min(prev + LOAD_MORE_COUNT, messages.length),
			);
			// After state update, adjust scroll to maintain position
			requestAnimationFrame(() => {
				const newScrollHeight = container.scrollHeight;
				container.scrollTop = newScrollHeight - prevScrollHeight;
			});
		}

		const userScrolled = !isNearBottom;
		setIsUserScrolled(userScrolled);

		// Save scroll position to cache
		if (userScrolled) {
			setCachedScrollPosition(container.scrollTop);
		} else {
			// At bottom - clear saved position so next mount scrolls to bottom
			setCachedScrollPosition(null);
		}
	}, [visibleCount, messages.length]);

	// Focus input on mount - only on desktop to avoid opening keyboard on mobile
	useEffect(() => {
		// Check if device has a coarse pointer (touch) - indicates mobile
		const isTouchDevice = window.matchMedia("(pointer: coarse)").matches;
		if (!isTouchDevice) {
			inputRef.current?.focus();
		}
	}, []);

	// Auto-resize is now handled inline in handleInputChange for better performance.
	// This effect is only needed for programmatic input changes (e.g., dictation, file select).
	const lastInputLengthRef = useRef(input.length);
	useEffect(() => {
		// Keep the textarea stable during dictation; the DictationOverlay shows the transcript.
		if (dictation.isActive) {
			lastInputLengthRef.current = input.length;
			if (inputRef.current) {
				inputRef.current.style.height = "36px";
			}
			return;
		}

		// Only resize if the input changed programmatically (not via typing, which is handled inline)
		const currentLength = input.length;
		const lastLength = lastInputLengthRef.current;
		// Skip if this is likely a single character change (typing)
		if (Math.abs(currentLength - lastLength) <= 1) {
			lastInputLengthRef.current = currentLength;
			return;
		}
		lastInputLengthRef.current = currentLength;
		const textarea = inputRef.current;
		if (textarea) {
			textarea.style.height = "auto";
			textarea.style.height = `${Math.min(textarea.scrollHeight, 200)}px`;
		}
	}, [dictation.isActive, input]);

	// Handle file upload
	const handleFileUpload = useCallback(
		async (files: FileList | null) => {
			if (!files || files.length === 0 || !workspacePath) return;

			setIsUploading(true);
			const baseUrl = fileserverWorkspaceBaseUrl();

			for (const file of Array.from(files)) {
				try {
					const formData = new FormData();
					formData.append("file", file);

					const uploadUrl = new URL(`${baseUrl}/file`, window.location.origin);
					uploadUrl.searchParams.set("path", file.name);
					uploadUrl.searchParams.set("workspace_path", workspacePath);

					const res = await fetch(uploadUrl.toString(), {
						method: "POST",
						body: formData,
						credentials: "include",
					});

					if (!res.ok) {
						console.error("Failed to upload file:", file.name);
						continue;
					}

					const attachment: FileAttachment = {
						id: `file-${Date.now()}-${Math.random().toString(36).slice(2)}`,
						path: file.name,
						filename: file.name,
						type: "file",
					};
					setFileAttachments((prev) => [...prev, attachment]);
				} catch (err) {
					console.error("Failed to upload file:", err);
				}
			}
			setIsUploading(false);
		},
		[workspacePath],
	);

	const handleModelChange = useCallback(
		async (value: string) => {
			const separatorIndex = value.indexOf("/");
			if (separatorIndex <= 0 || separatorIndex === value.length - 1) return;
			const provider = value.slice(0, separatorIndex);
			const modelId = value.slice(separatorIndex + 1);
			setSelectedModelRef(value);
			setIsSwitchingModel(true);
			try {
				await setMainChatPiModel(provider, modelId);
				await refresh();
			} catch (err) {
				console.error("Failed to switch model:", err);
			} finally {
				setIsSwitchingModel(false);
			}
		},
		[refresh],
	);

	const runSlashCommand = useCallback(
		async (command: string, args: string) => {
			setCommandError(null);
			const trimmedArgs = args.trim();
			const needsArgs = ["steer", "followup", "model"].includes(command);
			if (needsArgs && !trimmedArgs) {
				setInput(`/${command} `);
				inputRef.current?.focus();
				return { handled: true, clearInput: false };
			}

			switch (command) {
				case "compact": {
					await compactMainChatPi(trimmedArgs || undefined);
					await refresh();
					return { handled: true, clearInput: true };
				}
				case "new": {
					await newSession();
					return { handled: true, clearInput: true };
				}
				case "reset": {
					await resetSession();
					return { handled: true, clearInput: true };
				}
				case "abort": {
					await abort();
					return { handled: true, clearInput: true };
				}
				case "steer": {
					await send(trimmedArgs, { mode: "steer" });
					return { handled: true, clearInput: true };
				}
				case "followup": {
					await send(trimmedArgs, { mode: "follow_up" });
					return { handled: true, clearInput: true };
				}
				case "model": {
					const separatorIndex = trimmedArgs.indexOf("/");
					if (
						separatorIndex <= 0 ||
						separatorIndex === trimmedArgs.length - 1
					) {
						throw new Error("Model must be provider/model");
					}
					await handleModelChange(trimmedArgs);
					return { handled: true, clearInput: true };
				}
				default:
					return { handled: false, clearInput: false };
			}
		},
		[abort, handleModelChange, newSession, refresh, resetSession, send],
	);

	const handleSend = useCallback(
		async (mode: "steer" | "follow_up" = "steer") => {
			const trimmed = input.trim();
			if (!trimmed && fileAttachments.length === 0) return;

			if (slashQuery.isSlash && builtInCommandNames.has(slashQuery.command)) {
				try {
					const result = await runSlashCommand(
						slashQuery.command,
						slashQuery.args,
					);
					if (result.handled) {
						if (result.clearInput) {
							setInput("");
							setFileAttachments([]);
							if (inputRef.current) {
								inputRef.current.style.height = "auto";
							}
						}
						setShowSlashPopup(false);
						return;
					}
				} catch (err) {
					setCommandError(
						err instanceof Error ? err : new Error("Command failed"),
					);
					return;
				}
			}

			// Check for shell command (starts with "!")
			const isShellCommand = trimmed.startsWith("!");
			const shellCommand = isShellCommand ? trimmed.slice(1).trim() : "";

			// Build message with file attachments
			let message = trimmed;
			if (isShellCommand && shellCommand) {
				// Convert shell command to a prompt for Pi's bash tool
				message = `Run this shell command and show me the output:\n\`\`\`bash\n${shellCommand}\n\`\`\``;
			} else if (fileAttachments.length > 0) {
				const fileRefs = fileAttachments.map((f) => `@${f.path}`).join(" ");
				message = `${fileRefs}\n\n${trimmed}`;
			}

			setInput("");
			setFileAttachments([]);
			// Clear draft from localStorage
			try {
				localStorage.removeItem("octo:mainChatDraft");
			} catch {
				// Ignore localStorage errors
			}
			// Reset textarea height
			if (inputRef.current) {
				inputRef.current.style.height = "auto";
			}
			await send(message, { mode });
		},
		[
			builtInCommandNames,
			fileAttachments,
			input,
			runSlashCommand,
			send,
			slashQuery.command,
			slashQuery.args,
			slashQuery.isSlash,
		],
	);

	const handleKeyDown = useCallback(
		(e: KeyboardEvent<HTMLTextAreaElement>) => {
			if (showSlashPopup && slashQuery.isSlash && !slashQuery.args) {
				if (
					["ArrowDown", "ArrowUp", "Enter", "Tab", "Escape"].includes(e.key)
				) {
					return;
				}
			}
			// Let file mention popup handle its keys
			if (showFileMentionPopup) {
				if (
					["ArrowDown", "ArrowUp", "Enter", "Tab", "Escape"].includes(e.key)
				) {
					return;
				}
			}
			if (e.key === "Enter" && !e.shiftKey) {
				e.preventDefault();
				handleSend(e.ctrlKey || e.metaKey ? "follow_up" : "steer");
			}
			if (e.key === "Escape") {
				setShowFileMentionPopup(false);
				setShowSlashPopup(false);
			}
		},
		[handleSend, showFileMentionPopup, showSlashPopup, slashQuery],
	);

	const handleInputChange = useCallback(
		(e: ChangeEvent<HTMLTextAreaElement>) => {
			const textarea = e.target;
			const value = textarea.value;
			setInput(value);
			setCommandError(null);

			// Keep textarea stable during dictation to avoid reflow storms.
			if (dictation.isActive) {
				textarea.style.height = "36px";
			} else {
				// Auto-resize textarea immediately
				textarea.style.height = "auto";
				textarea.style.height = `${Math.min(textarea.scrollHeight, 200)}px`;
			}

			// Debounce draft persistence to localStorage (300ms)
			if (draftSaveTimeoutRef.current) {
				clearTimeout(draftSaveTimeoutRef.current);
			}
			draftSaveTimeoutRef.current = setTimeout(() => {
				try {
					if (value.trim()) {
						localStorage.setItem("octo:mainChatDraft", value);
					} else {
						localStorage.removeItem("octo:mainChatDraft");
					}
				} catch {
					// Ignore localStorage errors
				}
			}, 300);

			if (value.startsWith("/")) {
				setShowSlashPopup(true);
				setShowFileMentionPopup(false);
				setFileMentionQuery("");
				return;
			}

			setShowSlashPopup(false);

			// Show file mention popup when typing @
			const atMatch = value.match(/@[^\s]*$/);
			if (atMatch) {
				setShowFileMentionPopup(true);
				setFileMentionQuery(atMatch[1]);
			} else {
				setShowFileMentionPopup(false);
				setFileMentionQuery("");
			}
		},
		[dictation.isActive],
	);

	const handleFileSelect = useCallback((file: FileAttachment) => {
		setFileAttachments((prev) => [...prev, file]);
		// Remove the @query from input
		setInput((prev) => prev.replace(/@[^\s]*$/, ""));
		setShowFileMentionPopup(false);
		setFileMentionQuery("");
		inputRef.current?.focus();
	}, []);

	const handleStop = useCallback(async () => {
		await abort();
	}, [abort]);

	// Voice mode handlers
	const handleVoiceConversation = useCallback(() => {
		setVoiceMode("conversation");
		dictation.start();
	}, [dictation]);

	const handleVoiceDictation = useCallback(async () => {
		setVoiceMode("dictation");
		await dictation.start();
	}, [dictation]);

	const handleVoiceStop = useCallback(() => {
		// Use cancel() to stop without auto-send - user clicked X
		dictation.cancel();
		setVoiceMode(null);
	}, [dictation]);

	const hasVoice = !!features?.voice;

	const t = useMemo(
		() =>
			locale === "de"
				? {
						noMessages: "Noch keine Nachrichten",
						inputPlaceholder: "Nachricht eingeben...",
						send: "Senden",
						agentWorking: "Agent arbeitet...",
						stopAgent: "Agent stoppen",
						uploadFile: "Datei hochladen",
						speakNow: "Sprechen Sie...",
					}
				: {
						noMessages: "No messages yet",
						inputPlaceholder: "Type a message...",
						send: "Send",
						agentWorking: "Agent working...",
						stopAgent: "Stop agent",
						uploadFile: "Upload file",
						speakNow: "Speak now...",
					},
		[locale],
	);

	return (
		<div className={cn("flex flex-col h-full min-h-0", className)}>
			{!hideHeader && (
				<div className="pb-3 mb-3 border-b border-border">
					<div className="min-w-0 flex-1">
						<h1 className="text-base sm:text-lg font-semibold text-foreground tracking-wider truncate">
							{assistantName || (locale === "de" ? "Hauptchat" : "Main Chat")}
						</h1>
						{workspacePath && (
							<div className="flex items-center gap-2 text-xs text-foreground/60 dark:text-muted-foreground">
								<span className="font-mono">
									{workspacePath.split("/").pop()}
								</span>
							</div>
						)}
					</div>
					<div className="mt-2">
						<ContextWindowGauge
							inputTokens={gaugeTokens.inputTokens}
							outputTokens={gaugeTokens.outputTokens}
							maxTokens={contextWindowLimit}
							locale={locale}
							compact
						/>
					</div>
				</div>
			)}

			{/* Error banner - only show if no cached messages available */}
			{displayError && messages.length === 0 && (
				<div className="px-4 py-2 bg-destructive/10 text-destructive text-sm">
					{displayError.message}
				</div>
			)}

			{/* Working indicator with stop button */}
			{isStreaming && (
				<div className="flex items-center gap-1.5 px-2 py-0.5 bg-primary/10 text-xs text-primary">
					<BrailleSpinner />
					<span className="font-medium flex-1">{t.agentWorking}</span>
					<button
						type="button"
						onClick={handleStop}
						className="mr-1 text-destructive hover:text-destructive/80 transition-colors"
						title={t.stopAgent}
					>
						<StopCircle className="w-5 h-5" />
					</button>
				</div>
			)}

			{/* Messages area */}
			<div className="relative flex-1 min-h-0">
				<div
					ref={messagesContainerRef}
					onScroll={handleScroll}
					className="h-full bg-muted/30 border border-border p-2 sm:p-4 overflow-y-auto scrollbar-hide"
				>
					{messages.length === 0 && (
						<div className="text-sm text-muted-foreground">{t.noMessages}</div>
					)}

					{/* Load more indicator */}
					{messages.length > visibleCount && (
						<div className="text-center text-xs text-muted-foreground py-2">
							{messages.length - visibleCount} older messages...
						</div>
					)}

					{/* Only render the last visibleCount messages for performance */}
					{messages.slice(-visibleCount).map((message, index) => (
						<div key={message.id} className={index > 0 ? "mt-4 sm:mt-6" : ""}>
							<PiMessageCard
								message={message}
								locale={locale}
								workspacePath={workspacePath}
								assistantName={assistantName}
								a2uiSurfaces={surfacesByMessageId.get(message.id)}
								onA2UIAction={handleA2UIAction}
							/>
						</div>
					))}

					{/* Orphaned A2UI surfaces (no valid anchor) */}
					{orphanedSurfaces.length > 0 && (
						<div className="space-y-2 mt-4">
							{orphanedSurfaces.map((surface) => (
								<A2UICallCard
									key={surface.surfaceId}
									surfaceId={surface.surfaceId}
									messages={surface.messages}
									blocking={surface.blocking}
									requestId={surface.requestId}
									answered={surface.answered}
									answeredAction={surface.answeredAction}
									answeredAt={surface.answeredAt}
									onAction={handleA2UIAction}
								/>
							))}
						</div>
					)}

					<div ref={messagesEndRef} />
				</div>
			</div>

			{/* Hidden file input */}
			<input
				ref={fileInputRef}
				type="file"
				multiple
				className="hidden"
				onChange={(e) => handleFileUpload(e.target.files)}
			/>

			{/* Chat input - matches OpenCode style exactly */}
			<div className="chat-input-container flex flex-col gap-1 bg-muted/30 border border-border px-2 py-1 mt-2">
				<div className="flex items-center gap-2">
					{/* File upload button */}
					<button
						type="button"
						onClick={() => fileInputRef.current?.click()}
						disabled={isUploading}
						className="flex-shrink-0 h-8 px-2 flex items-center justify-center text-muted-foreground hover:text-foreground disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
						title={t.uploadFile}
					>
						{isUploading ? (
							<Loader2 className="size-4 animate-spin" />
						) : (
							<Paperclip className="size-4" />
						)}
					</button>

					{/* Voice menu button */}
					{hasVoice && (
						<VoiceMenuButton
							activeMode={voiceMode}
							voiceState={dictation.isActive ? "listening" : "idle"}
							onConversation={handleVoiceConversation}
							onDictation={handleVoiceDictation}
							onStop={handleVoiceStop}
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
							onSelect={(cmd) => {
								if (builtInCommandNames.has(cmd.name)) {
									runSlashCommand(cmd.name, slashQuery.args)
										.then((result) => {
											if (result.clearInput) {
												setInput("");
												setFileAttachments([]);
												if (inputRef.current) {
													inputRef.current.style.height = "auto";
												}
											}
											setShowSlashPopup(false);
										})
										.catch((err) => {
											setCommandError(
												err instanceof Error
													? err
													: new Error("Command failed"),
											);
											setShowSlashPopup(false);
										});
									return;
								}

								if (slashQuery.args.trim()) {
									send(`/${cmd.name} ${slashQuery.args.trim()}`)
										.then(() => {
											setInput("");
											setFileAttachments([]);
											if (inputRef.current) {
												inputRef.current.style.height = "auto";
											}
											setShowSlashPopup(false);
										})
										.catch((err) => {
											setCommandError(
												err instanceof Error
													? err
													: new Error("Command failed"),
											);
											setShowSlashPopup(false);
										});
								} else {
									setInput(`/${cmd.name} `);
									inputRef.current?.focus();
									setShowSlashPopup(false);
								}
							}}
							onClose={() => setShowSlashPopup(false)}
						/>
						<FileMentionPopup
							query={fileMentionQuery}
							isOpen={showFileMentionPopup}
							workspacePath={workspacePath ?? null}
							onSelect={handleFileSelect}
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

						{hasVoice && dictation.isActive ? (
							<DictationOverlay
								open
								value={input}
								liveTranscript={dictation.liveTranscript}
								placeholder={t.speakNow}
								vadProgress={dictation.vadProgress}
								autoSend={dictation.autoSendEnabled}
								onAutoSendChange={dictation.setAutoSendEnabled}
								onStop={handleVoiceStop}
								onChange={handleInputChange}
								onKeyDown={handleKeyDown}
								onPaste={(e) => {
									// Handle pasted files
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
										e.preventDefault();
										const dataTransfer = new DataTransfer();
										for (const file of files) {
											dataTransfer.items.add(file);
										}
										handleFileUpload(dataTransfer.files);
									}
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
								ref={inputRef}
								autoComplete="off"
								autoCorrect="off"
								autoCapitalize="sentences"
								spellCheck={false}
								enterKeyHint="send"
								data-form-type="other"
								placeholder={t.inputPlaceholder}
								value={input}
								onChange={handleInputChange}
								onKeyDown={handleKeyDown}
								onPaste={(e) => {
									// Handle pasted files
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
										e.preventDefault();
										const dataTransfer = new DataTransfer();
										for (const file of files) {
											dataTransfer.items.add(file);
										}
										handleFileUpload(dataTransfer.files);
									}
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
								rows={1}
								className="w-full bg-transparent border-none outline-none text-foreground placeholder:text-muted-foreground text-sm resize-none py-1.5 leading-5 max-h-[200px] overflow-y-auto"
							/>
						)}
					</div>

					{/* Send button */}
					<Button
						type="button"
						data-dictation-send
						onClick={() => handleSend("steer")}
						disabled={!input.trim() && fileAttachments.length === 0}
						className="flex-shrink-0 h-8 px-2 flex items-center justify-center text-muted-foreground hover:text-foreground disabled:opacity-50 disabled:cursor-not-allowed transition-colors p-0 bg-transparent hover:bg-transparent"
						variant="ghost"
						size="icon"
					>
						<Send className="w-4 h-4" />
					</Button>
				</div>
			</div>
		</div>
	);
}

/**
 * Renders a single Pi message - styled to match OpenCode MessageGroupCard exactly.
 */
const PiMessageCard = memo(function PiMessageCard({
	message,
	locale,
	workspacePath,
	assistantName,
	a2uiSurfaces,
	onA2UIAction,
}: {
	message: PiDisplayMessage;
	locale: "en" | "de";
	workspacePath?: string | null;
	assistantName?: string | null;
	a2uiSurfaces?: A2UISurfaceState[];
	onA2UIAction?: (action: import("@/lib/a2ui/types").A2UIUserAction) => void;
}) {
	const isUser = message.role === "user";
	const isSystem = message.role === "system";

	// Handle system messages (separators) differently
	if (isSystem) {
		const separatorPart = message.parts[0];
		const sessionId =
			separatorPart?.type === "separator" ? separatorPart.sessionId : undefined;
		return (
			<div className="flex items-center gap-4 py-2" data-session-id={sessionId}>
				<div className="flex-1 h-px bg-border" />
				<span className="text-xs text-muted-foreground px-2">
					{separatorPart?.type === "separator"
						? separatorPart.content
						: locale === "de"
							? "Neue Unterhaltung"
							: "New conversation"}
				</span>
				<div className="flex-1 h-px bg-border" />
			</div>
		);
	}

	const textContent = message.parts
		.filter(
			(p): p is Extract<PiMessagePart, { type: "text" }> => p.type === "text",
		)
		.map((p) => p.content)
		.join("\n");

	const createdAt = message.timestamp ? new Date(message.timestamp) : null;

	// Use configured assistant name or fallback to "Assistant"
	const displayName = isUser ? "You" : assistantName || "Assistant";

	const messageCard = (
		<div
			data-message-id={message.id}
			className={cn(
				"group transition-all duration-200 overflow-hidden",
				isUser
					? "sm:ml-8 bg-primary/20 dark:bg-primary/10 border border-primary/40 dark:border-primary/30"
					: "sm:mr-8 bg-muted/50 border border-border",
			)}
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
				) : (
					<Bot className="w-3 h-3 sm:w-4 sm:h-4 text-primary flex-shrink-0" />
				)}
				<span className="text-sm font-medium text-foreground">
					{displayName}
				</span>
				<div className="flex-1" />
				{/* Read aloud button for assistant messages */}
				{!isUser && textContent && !message.isStreaming && (
					<ReadAloudButton text={textContent} className="ml-1" />
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
				{/* Copy button - to the right of timestamp */}
				{textContent && !message.isStreaming && (
					<CopyButton
						text={textContent}
						className="ml-1 [&_svg]:w-3 [&_svg]:h-3"
					/>
				)}
			</div>

			{/* Content */}
			<div className="px-2 sm:px-4 py-2 sm:py-3 group space-y-3 overflow-hidden">
				{message.parts.length === 0 && !isUser && message.isStreaming && (
					<div className="flex items-center gap-3 text-muted-foreground text-sm">
						<BrailleSpinner />
						<span>{locale === "de" ? "Arbeitet..." : "Working..."}</span>
					</div>
				)}

				{/* Render parts, combining tool_use with matching tool_result */}
				{(() => {
					// Build a map of tool_result by id for quick lookup
					const toolResults = new Map<string, PiMessagePart>();
					for (const part of message.parts) {
						if (part.type === "tool_result") {
							toolResults.set(part.id, part);
						}
					}
					// Track which tool_results we've already rendered
					const renderedResults = new Set<string>();

					return message.parts.map((part, idx) => {
						// Skip tool_result if it will be rendered with its tool_use
						if (part.type === "tool_result" && renderedResults.has(part.id)) {
							return null;
						}

						// For tool_use, find and combine with matching tool_result
						if (part.type === "tool_use") {
							const result = toolResults.get(part.id);
							if (result && result.type === "tool_result") {
								renderedResults.add(part.id);
							}
							return (
								<PiPartRenderer
									key={`${message.id}-part-${idx}`}
									part={part}
									toolResult={
										result?.type === "tool_result" ? result : undefined
									}
									locale={locale}
									workspacePath={workspacePath}
								/>
							);
						}

						return (
							<PiPartRenderer
								key={`${message.id}-part-${idx}`}
								part={part}
								locale={locale}
								workspacePath={workspacePath}
							/>
						);
					});
				})()}

				{/* A2UI surfaces anchored to this message */}
				{a2uiSurfaces && a2uiSurfaces.length > 0 && (
					<div className="space-y-2 mt-2">
						{a2uiSurfaces.map((surface) => (
							<A2UICallCard
								key={surface.surfaceId}
								surfaceId={surface.surfaceId}
								messages={surface.messages}
								blocking={surface.blocking}
								requestId={surface.requestId}
								answered={surface.answered}
								answeredAction={surface.answeredAction}
								answeredAt={surface.answeredAt}
								onAction={onA2UIAction}
								defaultCollapsed={surface.answered}
							/>
						))}
					</div>
				)}

				{/* Streaming indicator when there's already content */}
				{message.isStreaming && message.parts.length > 0 && (
					<div className="flex items-center gap-3 text-muted-foreground text-sm">
						<BrailleSpinner />
						<span>{locale === "de" ? "Arbeitet..." : "Working..."}</span>
					</div>
				)}

				{/* Usage info */}
				{message.usage && !message.isStreaming && (
					<div className="text-xs text-muted-foreground pt-2 border-t border-border/50">
						{message.usage.input + message.usage.output} tokens
						{message.usage.cost?.total !== undefined && (
							<span className="ml-2">
								${message.usage.cost.total.toFixed(4)}
							</span>
						)}
					</div>
				)}
			</div>
		</div>
	);

	// Only wrap in context menu if there's text content to copy
	if (!textContent) {
		return messageCard;
	}

	return (
		<ContextMenu>
			<ContextMenuTrigger className="contents">{messageCard}</ContextMenuTrigger>
			<ContextMenuContent>
				<ContextMenuItem
					onClick={() => navigator.clipboard?.writeText(textContent)}
					className="gap-2"
				>
					<Copy className="w-4 h-4" />
					{locale === "de" ? "Alles kopieren" : "Copy all"}
				</ContextMenuItem>
			</ContextMenuContent>
		</ContextMenu>
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
}: {
	part: PiMessagePart;
	toolResult?: {
		type: "tool_result";
		id: string;
		name?: string;
		content: unknown;
		isError?: boolean;
	};
	locale: "en" | "de";
	workspacePath?: string | null;
}) {
	switch (part.type) {
		case "text":
			return (
				<TextWithFileReferences
					content={part.content}
					workspacePath={workspacePath}
					locale={locale}
				/>
			);

		case "thinking":
			return (
				<details className="text-xs text-muted-foreground border-l-2 border-muted pl-2 my-2">
					<summary className="cursor-pointer hover:text-foreground">
						{locale === "de" ? "Gedanken" : "Thinking"}
					</summary>
					<pre className="mt-1 whitespace-pre-wrap font-mono text-xs">
						{part.content}
					</pre>
				</details>
			);

		case "tool_use":
			return (
				<ToolCallCard
					part={{
						id: part.id,
						sessionID: "",
						messageID: "",
						type: "tool",
						tool: part.name,
						callID: part.id,
						state: {
							status: toolResult ? "completed" : "running",
							input: part.input as Record<string, unknown>,
							output: toolResult
								? typeof toolResult.content === "string"
									? toolResult.content
									: JSON.stringify(toolResult.content)
								: undefined,
							title: part.name,
						},
					}}
					defaultCollapsed={true}
					hideTodoTools={false}
				/>
			);

		case "tool_result":
			// Tool results are now rendered together with tool_use
			// Only render standalone if there's no matching tool_use
			return (
				<ToolCallCard
					part={{
						id: part.id,
						sessionID: "",
						messageID: "",
						type: "tool",
						tool: part.name || "result",
						callID: part.id,
						state: {
							status: "completed",
							output:
								typeof part.content === "string"
									? part.content
									: JSON.stringify(part.content),
							title: part.name || "Tool Result",
						},
					}}
					defaultCollapsed={true}
					hideTodoTools={false}
				/>
			);

		case "separator":
			return (
				<div className="text-xs text-muted-foreground italic">
					{part.content}
				</div>
			);

		default: {
			console.warn("Unknown Pi message part type:", part);
			return null;
		}
	}
}

/**
 * Renders text content with @file references as inline previews.
 * Wrapped in context menu for copy functionality on mobile.
 */
function TextWithFileReferences({
	content,
	workspacePath,
	locale = "en",
}: {
	content: string;
	workspacePath?: string | null;
	locale?: "en" | "de";
}) {
	// Parse @file references, excluding code blocks
	const fileRefs = useMemo(
		() => extractFileReferences(content),
		[content],
	);

	return (
		<ContextMenu>
			<ContextMenuTrigger className="contents">
				<div className="space-y-2 select-none sm:select-auto">
					<MarkdownRenderer
						content={content}
						className="text-sm leading-relaxed"
					/>
					{/* Render file reference cards */}
					{fileRefs.length > 0 && workspacePath && (
						<div className="flex flex-wrap gap-2 mt-2">
							{fileRefs.map((filePath) => (
								<FileReferenceCard
									key={filePath}
									filePath={filePath}
									workspacePath={workspacePath}
								/>
							))}
						</div>
					)}
				</div>
			</ContextMenuTrigger>
			<ContextMenuContent>
				<ContextMenuItem
					onClick={() => navigator.clipboard?.writeText(content)}
					className="gap-2"
				>
					<Copy className="w-4 h-4" />
					{locale === "de" ? "Kopieren" : "Copy"}
				</ContextMenuItem>
			</ContextMenuContent>
		</ContextMenu>
	);
}

/**
 * Card for displaying a @file reference with preview.
 * Only renders if the file exists on disk.
 */
export const FileReferenceCard = memo(function FileReferenceCard({
	filePath,
	workspacePath,
}: {
	filePath: string;
	workspacePath: string;
}) {
	const [imageError, setImageError] = useState(false);
	const [isLoading, setIsLoading] = useState(true);
	const [fileExists, setFileExists] = useState<boolean | null>(null);

	const fileInfo = useMemo(() => getFileTypeInfo(filePath), [filePath]);
	const isImage = fileInfo.category === "image";
	const isVideo = fileInfo.category === "video";

	const fileUrl = workspaceFileUrl(workspacePath, filePath);

	// Check if file exists using HEAD request
	useEffect(() => {
		let cancelled = false;
		fetch(fileUrl, { method: "HEAD" })
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

	// Don't render anything if file doesn't exist
	if (fileExists === false) {
		return null;
	}

	// Show loading state while checking existence
	if (fileExists === null) {
		return null;
	}

	if (isImage && !imageError) {
		return (
			<div className="relative inline-block rounded-lg overflow-hidden border border-border bg-muted/50 max-w-[200px]">
				{isLoading && (
					<div className="absolute inset-0 flex items-center justify-center bg-muted">
						<Loader2 className="w-4 h-4 animate-spin text-muted-foreground" />
					</div>
				)}
				<img
					src={fileUrl}
					alt={filePath}
					className="max-w-full h-auto max-h-[150px] object-contain"
					onLoad={() => setIsLoading(false)}
					onError={() => {
						setImageError(true);
						setIsLoading(false);
					}}
				/>
				<div className="absolute bottom-0 left-0 right-0 bg-black/60 text-white text-xs px-2 py-1 truncate">
					{filePath.split("/").pop()}
				</div>
			</div>
		);
	}

	// Video preview
	if (isVideo && !imageError) {
		return (
			<div className="relative inline-block rounded-lg overflow-hidden border border-border bg-muted/50 max-w-[200px]">
				<video
					src={fileUrl}
					controls
					playsInline
					className="max-w-full h-auto max-h-[150px] object-contain"
					onLoadedData={() => setIsLoading(false)}
					onError={() => {
						setImageError(true);
						setIsLoading(false);
					}}
				>
					<track kind="captions" />
				</video>
				<div className="absolute bottom-0 left-0 right-0 bg-black/60 text-white text-xs px-2 py-1 truncate flex items-center gap-1">
					<FileVideo className="w-3 h-3" />
					{filePath.split("/").pop()}
				</div>
			</div>
		);
	}

	// Non-image/video or load error - show compact link
	const Icon = isImage ? ImageIcon : File;

	return (
		<a
			href={fileUrl}
			target="_blank"
			rel="noopener noreferrer"
			className="inline-flex items-center gap-1.5 px-2 py-1 rounded-md bg-muted/50 border border-border text-xs hover:bg-muted transition-colors"
		>
			<Icon className="w-3.5 h-3.5 text-muted-foreground" />
			<span className="truncate max-w-[150px]">
				{filePath.split("/").pop()}
			</span>
			<ExternalLink className="w-3 h-3 text-muted-foreground" />
		</a>
	);
});
