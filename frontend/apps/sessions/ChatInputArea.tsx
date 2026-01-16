"use client";

import { Button } from "@/components/ui/button";
import {
	type FileAttachment,
	FileAttachmentChip,
	FileMentionPopup,
} from "@/components/ui/file-mention-popup";
import { SlashCommandPopup } from "@/components/ui/slash-command-popup";
import {
	DictationOverlay,
	VoiceMenuButton,
	type VoiceMode,
} from "@/components/voice";
import type { UseDictationReturn } from "@/hooks/use-dictation";
import type { Features } from "@/lib/control-plane-client";
import {
	type SlashCommand,
	builtInCommands,
	parseSlashInput,
} from "@/lib/slash-commands";
import { cn } from "@/lib/utils";
import {
	Loader2,
	Mic,
	Paperclip,
	RefreshCw,
	Send,
	StopCircle,
	X,
} from "lucide-react";
import {
	forwardRef,
	memo,
	startTransition,
	useCallback,
	useDeferredValue,
	useEffect,
	useImperativeHandle,
	useMemo,
	useRef,
	useState,
} from "react";

export interface PendingUpload {
	file: File;
	path: string;
	previewUrl?: string;
}

export interface ChatInputAreaRef {
	getValue: () => string;
	setValue: (value: string) => void;
	getFileAttachments: () => FileAttachment[];
	getPendingUploads: () => PendingUpload[];
	clear: () => void;
	focus: () => void;
}

interface ChatInputAreaProps {
	locale: "de" | "en";
	placeholder?: string;
	disabled?: boolean;
	isLoading?: boolean;
	canResume?: boolean;
	features?: Features | null;
	dictation: UseDictationReturn;
	slashCommands: SlashCommand[];
	workspaceDirectory?: string;
	onSend: (
		message: string,
		fileAttachments: FileAttachment[],
		pendingUploads: PendingUpload[],
	) => void;
	onResume?: () => void;
	onAbort?: () => void;
	onFileUpload?: (files: FileList) => Promise<PendingUpload[]>;
	onDraftChange?: (value: string) => void;
	initialValue?: string;
	voiceMode?: VoiceMode;
	onVoiceModeChange?: (mode: VoiceMode) => void;
	onVoiceMenuClick?: () => void;
}

export const ChatInputArea = memo(
	forwardRef<ChatInputAreaRef, ChatInputAreaProps>(function ChatInputArea(
		{
			locale,
			placeholder,
			disabled,
			isLoading,
			canResume,
			features,
			dictation,
			slashCommands,
			workspaceDirectory,
			onSend,
			onResume,
			onAbort,
			onFileUpload,
			onDraftChange,
			initialValue = "",
			voiceMode,
			onVoiceModeChange,
			onVoiceMenuClick,
		},
		ref,
	) {
		// Input state - isolated from parent
		const [messageInput, setMessageInput] = useState(initialValue);
		const [showSlashPopup, setShowSlashPopup] = useState(false);
		const [showFileMentionPopup, setShowFileMentionPopup] = useState(false);
		const [fileMentionQuery, setFileMentionQuery] = useState("");
		const [fileAttachments, setFileAttachments] = useState<FileAttachment[]>(
			[],
		);
		const [pendingUploads, setPendingUploads] = useState<PendingUpload[]>([]);
		const [isUploading, setIsUploading] = useState(false);

		const chatInputRef = useRef<HTMLTextAreaElement>(null);
		const messageInputRef = useRef(messageInput);

		// Keep ref in sync
		useEffect(() => {
			messageInputRef.current = messageInput;
		}, [messageInput]);

		// Deferred value for non-critical computations
		const deferredMessageInput = useDeferredValue(messageInput);

		// Parse slash command from input
		const slashQuery = useMemo(
			() => parseSlashInput(deferredMessageInput),
			[deferredMessageInput],
		);

		// Resize helper
		const chatInputResizeRef = useRef<{
			raf: number | null;
			value: string;
		}>({ raf: null, value: "" });

		const setMessageInputWithResize = useCallback((value: string) => {
			setMessageInput(value);
			chatInputResizeRef.current.value = value;

			if (chatInputResizeRef.current.raf !== null) return;

			chatInputResizeRef.current.raf = requestAnimationFrame(() => {
				chatInputResizeRef.current.raf = null;
				const textarea = chatInputRef.current;
				if (!textarea) return;

				const currentValue = chatInputResizeRef.current.value;
				textarea.style.height = "36px";
				if (currentValue) {
					textarea.style.height = `${Math.min(textarea.scrollHeight, 200)}px`;
				}
			});
		}, []);

		// Expose imperative handle for parent access
		useImperativeHandle(
			ref,
			() => ({
				getValue: () => messageInputRef.current,
				setValue: setMessageInputWithResize,
				getFileAttachments: () => fileAttachments,
				getPendingUploads: () => pendingUploads,
				clear: () => {
					setMessageInputWithResize("");
					setFileAttachments([]);
					setPendingUploads([]);
				},
				focus: () => chatInputRef.current?.focus(),
			}),
			[fileAttachments, pendingUploads, setMessageInputWithResize],
		);

		// Draft change notification (debounced in parent)
		const draftTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);
		useEffect(() => {
			if (!onDraftChange) return;
			if (draftTimeoutRef.current) {
				clearTimeout(draftTimeoutRef.current);
			}
			draftTimeoutRef.current = setTimeout(() => {
				onDraftChange(messageInput);
			}, 300);
			return () => {
				if (draftTimeoutRef.current) {
					clearTimeout(draftTimeoutRef.current);
				}
			};
		}, [messageInput, onDraftChange]);

		// Handle input change
		const handleInputChange = useCallback(
			(e: React.ChangeEvent<HTMLTextAreaElement>) => {
				const value = e.target.value;

				if (dictation.isActive) {
					setMessageInput(value);
					e.target.style.height = "36px";
				} else {
					setMessageInputWithResize(value);
				}

				// Defer popup state updates
				startTransition(() => {
					if (value.startsWith("/")) {
						setShowSlashPopup(true);
						setShowFileMentionPopup(false);
					} else {
						setShowSlashPopup(false);
					}

					const atMatch = value.match(/@([^\s]*)$/);
					if (atMatch && !value.startsWith("/")) {
						setShowFileMentionPopup(true);
						setFileMentionQuery(atMatch[1]);
					} else {
						setShowFileMentionPopup(false);
						setFileMentionQuery("");
					}
				});
			},
			[dictation.isActive, setMessageInputWithResize],
		);

		// Handle send
		const handleSend = useCallback(() => {
			const text = messageInput.trim();
			if (
				!text &&
				pendingUploads.length === 0 &&
				fileAttachments.length === 0
			) {
				return;
			}

			if (dictation.isActive) {
				dictation.stop();
			}

			setShowSlashPopup(false);
			setShowFileMentionPopup(false);

			onSend(text, [...fileAttachments], [...pendingUploads]);

			// Clear input after send
			setMessageInputWithResize("");
			setFileAttachments([]);
			setPendingUploads([]);
		}, [
			messageInput,
			pendingUploads,
			fileAttachments,
			dictation,
			onSend,
			setMessageInputWithResize,
		]);

		// Handle key down
		const handleInputKeyDown = useCallback(
			(e: React.KeyboardEvent<HTMLTextAreaElement>) => {
				// Let popups handle their keys
				if (showSlashPopup && slashQuery.isSlash && !slashQuery.args) {
					if (
						["ArrowDown", "ArrowUp", "Enter", "Tab", "Escape"].includes(e.key)
					) {
						return;
					}
				}
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
					if (chatInputRef.current) {
						chatInputRef.current.style.height = "auto";
					}
				}
			},
			[
				showSlashPopup,
				slashQuery.isSlash,
				slashQuery.args,
				showFileMentionPopup,
				handleSend,
			],
		);

		// Handle file upload
		const handleFileUpload = useCallback(
			async (files: FileList) => {
				if (!onFileUpload) return;
				setIsUploading(true);
				try {
					const uploads = await onFileUpload(files);
					setPendingUploads((prev) => [...prev, ...uploads]);
				} finally {
					setIsUploading(false);
				}
			},
			[onFileUpload],
		);

		// Handle paste
		const handlePaste = useCallback(
			(e: React.ClipboardEvent<HTMLTextAreaElement>) => {
				const items = e.clipboardData?.items;
				if (!items) return;

				const files: File[] = [];
				let imageIndex = 0;
				for (const item of Array.from(items)) {
					if (item.kind === "file") {
						const file = item.getAsFile();
						if (file) {
							const isGenericName = /^image\.(png|gif|jpg|jpeg|webp)$/i.test(
								file.name,
							);
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
			},
			[handleFileUpload],
		);

		// Handle slash command select
		const handleSlashCommandSelect = useCallback(
			(command: SlashCommand) => {
				setMessageInputWithResize(`/${command.name} `);
				setShowSlashPopup(false);
				chatInputRef.current?.focus();
			},
			[setMessageInputWithResize],
		);

		// Handle file mention select
		const handleFileMentionSelect = useCallback(
			(file: FileAttachment) => {
				const newInput = messageInput.replace(/@[^\s]*$/, "");
				setMessageInputWithResize(newInput);
				setFileAttachments((prev) => [...prev, file]);
				setShowFileMentionPopup(false);
				chatInputRef.current?.focus();
			},
			[messageInput, setMessageInputWithResize],
		);

		const canSend =
			deferredMessageInput.trim() ||
			pendingUploads.length > 0 ||
			fileAttachments.length > 0;

		const t = useMemo(
			() => ({
				inputPlaceholder:
					locale === "de" ? "Nachricht eingeben..." : "Type a message...",
			}),
			[locale],
		);

		return (
			<div className="flex-shrink-0 p-2 sm:p-3 bg-background border-t border-border">
				{/* Pending uploads preview */}
				{pendingUploads.length > 0 && (
					<div className="flex flex-wrap gap-2 mb-2">
						{pendingUploads.map((upload) => (
							<div
								key={upload.path}
								className="relative group flex items-center gap-1.5 px-2 py-1 bg-muted border border-border text-xs"
							>
								{upload.previewUrl && (
									<img
										src={upload.previewUrl}
										alt={upload.file.name}
										className="w-6 h-6 object-cover"
									/>
								)}
								<span className="max-w-[150px] truncate">
									{upload.file.name}
								</span>
								<button
									type="button"
									onClick={() =>
										setPendingUploads((prev) =>
											prev.filter((u) => u.path !== upload.path),
										)
									}
									className="ml-1 text-muted-foreground hover:text-foreground"
								>
									<X className="w-3 h-3" />
								</button>
							</div>
						))}
					</div>
				)}

				{/* Input area */}
				<div className="flex items-end gap-2">
					<div className="flex-1 relative">
						{/* Slash command popup */}
						<SlashCommandPopup
							commands={slashCommands}
							query={slashQuery.command}
							isOpen={showSlashPopup && slashQuery.isSlash && !slashQuery.args}
							onSelect={handleSlashCommandSelect}
						/>

						{/* File mention popup */}
						<FileMentionPopup
							query={fileMentionQuery}
							isOpen={showFileMentionPopup}
							workspaceDirectory={workspaceDirectory}
							onSelect={handleFileMentionSelect}
						/>

						{/* File attachments */}
						{fileAttachments.length > 0 && (
							<div className="flex flex-wrap gap-1 mb-1">
								{fileAttachments.map((attachment) => (
									<FileAttachmentChip
										key={attachment.path}
										attachment={attachment}
										onRemove={() =>
											setFileAttachments((prev) =>
												prev.filter((a) => a.path !== attachment.path),
											)
										}
									/>
								))}
							</div>
						)}

						{/* Textarea or dictation overlay */}
						{dictation.isActive ? (
							<DictationOverlay
								value={messageInput}
								liveTranscript={dictation.liveTranscript}
								placeholder={
									locale === "de" ? "Sprechen Sie..." : "Speak now..."
								}
								vadProgress={dictation.vadProgress}
								autoSend={dictation.autoSendEnabled}
								onAutoSendChange={dictation.setAutoSendEnabled}
								onStop={() => {
									dictation.cancel();
									requestAnimationFrame(() => {
										setMessageInputWithResize(messageInputRef.current);
									});
								}}
								onChange={handleInputChange}
								onKeyDown={handleInputKeyDown}
								onPaste={handlePaste}
								onBlur={() => {
									setTimeout(() => setShowSlashPopup(false), 150);
								}}
								onFocus={(e) => {
									if (deferredMessageInput.startsWith("/")) {
										setShowSlashPopup(true);
									}
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
								ref={chatInputRef}
								autoComplete="off"
								autoCorrect="off"
								autoCapitalize="sentences"
								spellCheck={false}
								enterKeyHint="send"
								data-form-type="other"
								placeholder={placeholder ?? t.inputPlaceholder}
								value={messageInput}
								onChange={handleInputChange}
								onKeyDown={handleInputKeyDown}
								onPaste={handlePaste}
								onBlur={() => {
									setTimeout(() => setShowSlashPopup(false), 150);
								}}
								onFocus={(e) => {
									if (deferredMessageInput.startsWith("/")) {
										setShowSlashPopup(true);
									}
									setTimeout(() => {
										e.target.scrollIntoView({
											behavior: "smooth",
											block: "nearest",
										});
									}, 300);
								}}
								className="flex-1 w-full min-h-[36px] max-h-[200px] px-3 py-2 bg-muted border border-border text-sm resize-none placeholder:text-muted-foreground focus:outline-none focus:ring-1 focus:ring-ring scrollbar-hide"
								disabled={disabled}
							/>
						)}
					</div>

					{/* Action buttons */}
					<div className="flex items-center gap-1">
						{/* File upload button */}
						{onFileUpload && (
							<Button
								type="button"
								variant="ghost"
								size="icon"
								className="h-8 w-8"
								disabled={disabled || isUploading}
								onClick={() => {
									const input = document.createElement("input");
									input.type = "file";
									input.multiple = true;
									input.onchange = (e) => {
										const files = (e.target as HTMLInputElement).files;
										if (files) handleFileUpload(files);
									};
									input.click();
								}}
							>
								{isUploading ? (
									<Loader2 className="w-4 h-4 animate-spin" />
								) : (
									<Paperclip className="w-4 h-4" />
								)}
							</Button>
						)}

						{/* Voice button */}
						{features?.voice && (
							<VoiceMenuButton
								voiceMode={voiceMode ?? "off"}
								onVoiceModeChange={onVoiceModeChange ?? (() => {})}
								onMenuClick={onVoiceMenuClick}
								config={features.voice}
								dictationActive={dictation.isActive}
								onDictationToggle={() => {
									if (dictation.isActive) {
										dictation.stop();
									} else {
										dictation.start();
									}
								}}
							/>
						)}

						{/* Abort button */}
						{isLoading && onAbort && (
							<Button
								type="button"
								variant="ghost"
								size="icon"
								className="h-8 w-8 text-destructive"
								onClick={onAbort}
							>
								<StopCircle className="w-4 h-4" />
							</Button>
						)}

						{/* Send/Resume button */}
						<Button
							type="button"
							data-voice-send
							onClick={canResume && onResume ? onResume : handleSend}
							disabled={!canResume && !canSend}
							className="h-8 px-2"
							variant="ghost"
							size="icon"
						>
							{canResume ? (
								<RefreshCw className="w-4 h-4" />
							) : (
								<Send className="w-4 h-4" />
							)}
						</Button>
					</div>
				</div>
			</div>
		);
	}),
);

export default ChatInputArea;
