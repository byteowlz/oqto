"use client";

import { BrailleSpinner } from "@/components/ui/braille-spinner";
import { Button } from "@/components/ui/button";
import {
	type FileAttachment,
	FileAttachmentChip,
	FileMentionPopup,
} from "@/components/ui/file-mention-popup";
import {
	CopyButton,
	MarkdownRenderer,
} from "@/components/ui/markdown-renderer";
import { ReadAloudButton } from "@/components/ui/read-aloud-button";
import { ToolCallCard } from "@/components/ui/tool-call-card";
import {
	VoiceMenuButton,
	type VoiceMode,
} from "@/components/voice/VoiceMenuButton";
import { useDictation } from "@/hooks/use-dictation";
import {
	type PiDisplayMessage,
	type PiMessagePart,
	usePiChat,
} from "@/hooks/usePiChat";
import {
	type Features,
	fileserverWorkspaceBaseUrl,
} from "@/lib/control-plane-client";
import { getFileTypeInfo } from "@/lib/file-types";
import { cn } from "@/lib/utils";
import {
	Bot,
	ExternalLink,
	File,
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
}: MainChatPiViewProps) {
	const { messages, isConnected, isStreaming, error, send, abort } =
		usePiChat();

	const [input, setInput] = useState("");
	const [fileAttachments, setFileAttachments] = useState<FileAttachment[]>([]);
	const [showFileMentionPopup, setShowFileMentionPopup] = useState(false);
	const [fileMentionQuery, setFileMentionQuery] = useState("");
	const [voiceMode, setVoiceMode] = useState<VoiceMode>(null);
	const [isUploading, setIsUploading] = useState(false);

	const messagesEndRef = useRef<HTMLDivElement>(null);
	const inputRef = useRef<HTMLTextAreaElement>(null);
	const fileInputRef = useRef<HTMLInputElement>(null);

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

	// Dictation hook
	const dictation = useDictation({
		config: voiceConfig,
		onTranscript: useCallback((text: string) => {
			setInput((prev) => (prev ? `${prev} ${text}` : text));
		}, []),
		vadTimeoutMs: features?.voice?.vad_timeout_ms,
	});

	// Auto-scroll to bottom when new messages arrive
	useLayoutEffect(() => {
		if (messages.length > 0) {
			messagesEndRef.current?.scrollIntoView({ behavior: "smooth" });
		}
	});

	// Focus input on mount
	useEffect(() => {
		inputRef.current?.focus();
	}, []);

	// Auto-resize textarea - input dependency is intentional to trigger on text changes
	// biome-ignore lint/correctness/useExhaustiveDependencies: input triggers resize calculation
	useEffect(() => {
		const textarea = inputRef.current;
		if (textarea) {
			textarea.style.height = "auto";
			textarea.style.height = `${Math.min(textarea.scrollHeight, 200)}px`;
		}
	}, [input]);

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

	const handleSend = useCallback(async () => {
		const trimmed = input.trim();
		if ((!trimmed && fileAttachments.length === 0) || isStreaming) return;

		// Build message with file attachments
		let message = trimmed;
		if (fileAttachments.length > 0) {
			const fileRefs = fileAttachments.map((f) => `@${f.path}`).join(" ");
			message = `${fileRefs}\n\n${trimmed}`;
		}

		setInput("");
		setFileAttachments([]);
		// Reset textarea height
		if (inputRef.current) {
			inputRef.current.style.height = "auto";
		}
		await send(message);
	}, [input, fileAttachments, isStreaming, send]);

	const handleKeyDown = useCallback(
		(e: KeyboardEvent<HTMLTextAreaElement>) => {
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
				handleSend();
			}
			if (e.key === "Escape") {
				setShowFileMentionPopup(false);
			}
		},
		[handleSend, showFileMentionPopup],
	);

	const handleInputChange = useCallback(
		(e: ChangeEvent<HTMLTextAreaElement>) => {
			const value = e.target.value;
			setInput(value);

			// Show file mention popup when typing @
			const atMatch = value.match(/@([^\s]*)$/);
			if (atMatch) {
				setShowFileMentionPopup(true);
				setFileMentionQuery(atMatch[1]);
			} else {
				setShowFileMentionPopup(false);
				setFileMentionQuery("");
			}
		},
		[],
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
		dictation.stop();
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
			{/* Error banner */}
			{error && (
				<div className="px-4 py-2 bg-destructive/10 text-destructive text-sm">
					{error.message}
				</div>
			)}

			{/* Connection status */}
			{!isConnected && (
				<div className="px-4 py-2 bg-yellow-500/10 text-yellow-600 text-sm">
					{locale === "de" ? "Verbindung wird hergestellt..." : "Connecting..."}
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
				<div className="h-full bg-muted/30 border border-border p-2 sm:p-4 overflow-y-auto space-y-4 sm:space-y-6 scrollbar-hide">
					{messages.length === 0 && (
						<div className="text-sm text-muted-foreground">{t.noMessages}</div>
					)}

					{messages.map((message) => (
						<PiMessageCard
							key={message.id}
							message={message}
							locale={locale}
							workspacePath={workspacePath}
							assistantName={assistantName}
						/>
					))}

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
						disabled={isUploading || isStreaming}
						className="flex-shrink-0 size-8 flex items-center justify-center text-muted-foreground hover:text-foreground disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
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
							disabled={isStreaming}
							locale={locale}
							className="flex-shrink-0"
						/>
					)}

					{/* Textarea wrapper with file mention popup */}
					<div className="flex-1 relative flex flex-col min-h-[32px]">
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

						<textarea
							ref={inputRef}
							placeholder={
								dictation.isActive && dictation.liveTranscript
									? dictation.liveTranscript
									: dictation.isActive
										? t.speakNow
										: t.inputPlaceholder
							}
							value={input}
							onChange={handleInputChange}
							onKeyDown={handleKeyDown}
							onPaste={(e) => {
								// Handle pasted files
								const items = e.clipboardData?.items;
								if (!items) return;

								const files: File[] = [];
								for (const item of Array.from(items)) {
									if (item.kind === "file") {
										const file = item.getAsFile();
										if (file) files.push(file);
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
							disabled={isStreaming}
							className="w-full bg-transparent border-none outline-none text-foreground placeholder:text-muted-foreground text-sm resize-none py-1.5 leading-5 max-h-[200px] overflow-y-auto"
						/>
					</div>

					{/* Send button */}
					<Button
						type="button"
						onClick={handleSend}
						disabled={
							isStreaming || (!input.trim() && fileAttachments.length === 0)
						}
						className="bg-primary hover:bg-primary/90 text-primary-foreground"
					>
						<Send className="w-4 h-4 sm:mr-2" />
						<span className="hidden sm:inline">{t.send}</span>
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
}: {
	message: PiDisplayMessage;
	locale: "en" | "de";
	workspacePath?: string | null;
	assistantName?: string | null;
}) {
	const isUser = message.role === "user";
	const isSystem = message.role === "system";

	// Handle system messages (separators) differently
	if (isSystem) {
		return (
			<div className="flex items-center gap-4 py-2">
				<div className="flex-1 h-px bg-border" />
				<span className="text-xs text-muted-foreground px-2">
					{message.parts[0]?.type === "separator"
						? message.parts[0].content
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
		.join("\n\n");

	const createdAt = message.timestamp ? new Date(message.timestamp) : null;

	// Use configured assistant name or fallback to "Assistant"
	const displayName = isUser ? "You" : assistantName || "Assistant";

	return (
		<div
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

				{message.parts.map((part, idx) => (
					<PiPartRenderer
						key={`${message.id}-part-${idx}`}
						part={part}
						locale={locale}
						workspacePath={workspacePath}
					/>
				))}

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
});

/**
 * Renders a single part of a Pi message.
 */
function PiPartRenderer({
	part,
	locale,
	workspacePath,
}: {
	part: PiMessagePart;
	locale: "en" | "de";
	workspacePath?: string | null;
}) {
	switch (part.type) {
		case "text":
			return (
				<TextWithFileReferences
					content={part.content}
					workspacePath={workspacePath}
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
							status: "completed",
							input: part.input as Record<string, unknown>,
							title: part.name,
						},
					}}
					defaultCollapsed={true}
					hideTodoTools={false}
				/>
			);

		case "tool_result":
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
 */
function TextWithFileReferences({
	content,
	workspacePath,
}: {
	content: string;
	workspacePath?: string | null;
}) {
	// Parse @file references
	const fileRefPattern = /@([^\s@]+\.[a-zA-Z0-9]+)/g;
	const matches = content.match(fileRefPattern) || [];
	const fileRefs = [...new Set(matches.map((m) => m.slice(1)))]; // Remove @ prefix

	return (
		<div className="space-y-2">
			<MarkdownRenderer content={content} className="text-sm leading-relaxed" />
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
	);
}

/**
 * Card for displaying a @file reference with preview.
 */
const FileReferenceCard = memo(function FileReferenceCard({
	filePath,
	workspacePath,
}: {
	filePath: string;
	workspacePath: string;
}) {
	const [imageError, setImageError] = useState(false);
	const [isLoading, setIsLoading] = useState(true);

	const fileInfo = useMemo(() => getFileTypeInfo(filePath), [filePath]);
	const isImage = fileInfo.category === "image";

	const baseUrl = fileserverWorkspaceBaseUrl();
	const fileUrl = `${baseUrl}/read?path=${encodeURIComponent(filePath)}&workspace_path=${encodeURIComponent(workspacePath)}`;

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

	// Non-image or image load error - show compact link
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
