"use client";

import {
	type FileAttachment,
	FileAttachmentChip,
	FileMentionPopup,
	SlashCommandPopup,
} from "@/components/chat";
import { Button } from "@/components/ui/button";
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
	useEffect,
	useImperativeHandle,
	useMemo,
	useRef,
	useState,
} from "react";
import { useTranslation } from "react-i18next";

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
		const { t } = useTranslation();

		// ---------------------------------------------------------------
		// Input value lives in a ref (never in React state) so that
		// keystrokes never trigger a React render.  The textarea is
		// **uncontrolled** (uses defaultValue, not value).
		// ---------------------------------------------------------------
		const messageInputRef = useRef(initialValue);
		const chatInputRef = useRef<HTMLTextAreaElement>(null);

		// Slash-command parsing -- ref, no renders
		const slashQueryRef = useRef(parseSlashInput(initialValue));

		// Popup state -- these DO cause renders but are deferred via rAF+startTransition
		const [showSlashPopup, setShowSlashPopup] = useState(false);
		const [showFileMentionPopup, setShowFileMentionPopup] = useState(false);
		const [fileMentionQuery, setFileMentionQuery] = useState("");

		const [fileAttachments, setFileAttachments] = useState<FileAttachment[]>(
			[],
		);
		const [pendingUploads, setPendingUploads] = useState<PendingUpload[]>([]);
		const [isUploading, setIsUploading] = useState(false);

		// Track whether the send button should be enabled.
		// Updated via ref reads -- no per-keystroke state.
		const [canSendState, setCanSendState] = useState(!!initialValue.trim());

		// ---------------------------------------------------------------
		// Resize helper -- coalesces via rAF
		// ---------------------------------------------------------------
		const resizeRafRef = useRef<number | null>(null);

		const resizeTextarea = useCallback(() => {
			if (resizeRafRef.current !== null) return;
			resizeRafRef.current = requestAnimationFrame(() => {
				resizeRafRef.current = null;
				const textarea = chatInputRef.current;
				if (!textarea) return;
				textarea.style.height = "36px";
				if (textarea.value) {
					textarea.style.height = `${Math.min(textarea.scrollHeight, 200)}px`;
				}
			});
		}, []);

		// ---------------------------------------------------------------
		// Imperatively set the textarea value (for clear, slash select, etc.)
		// ---------------------------------------------------------------
		const setTextareaValue = useCallback(
			(value: string) => {
				messageInputRef.current = value;
				if (chatInputRef.current) {
					chatInputRef.current.value = value;
				}
				resizeTextarea();
			},
			[resizeTextarea],
		);

		// ---------------------------------------------------------------
		// Draft change notification (debounced 300ms)
		// ---------------------------------------------------------------
		const draftTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);
		const notifyDraft = useCallback(
			(value: string) => {
				if (!onDraftChange) return;
				if (draftTimeoutRef.current) clearTimeout(draftTimeoutRef.current);
				draftTimeoutRef.current = setTimeout(() => onDraftChange(value), 300);
			},
			[onDraftChange],
		);
		useEffect(() => {
			return () => {
				if (draftTimeoutRef.current) clearTimeout(draftTimeoutRef.current);
			};
		}, []);

		// ---------------------------------------------------------------
		// Deferred popup + canSend updater (rAF -> startTransition)
		// ---------------------------------------------------------------
		const popupRafRef = useRef<number | null>(null);

		const scheduleDeferredUpdates = useCallback((value: string) => {
			if (popupRafRef.current !== null)
				cancelAnimationFrame(popupRafRef.current);
			popupRafRef.current = requestAnimationFrame(() => {
				popupRafRef.current = null;
				startTransition(() => {
					// Slash popup
					if (value.startsWith("/")) {
						setShowSlashPopup(true);
						setShowFileMentionPopup(false);
					} else {
						setShowSlashPopup(false);
					}

					// File mention popup
					const atMatch = value.match(/@([^\s]*)$/);
					if (atMatch && !value.startsWith("/")) {
						setShowFileMentionPopup(true);
						setFileMentionQuery(atMatch[1]);
					} else {
						setShowFileMentionPopup(false);
						setFileMentionQuery("");
					}

					// Send-button enabled state
					setCanSendState(!!value.trim());
				});
			});
		}, []);

		// ---------------------------------------------------------------
		// Expose imperative handle for parent access
		// ---------------------------------------------------------------
		useImperativeHandle(
			ref,
			() => ({
				getValue: () => messageInputRef.current,
				setValue: setTextareaValue,
				getFileAttachments: () => fileAttachments,
				getPendingUploads: () => pendingUploads,
				clear: () => {
					setTextareaValue("");
					setFileAttachments([]);
					setPendingUploads([]);
					setCanSendState(false);
				},
				focus: () => chatInputRef.current?.focus(),
			}),
			[fileAttachments, pendingUploads, setTextareaValue],
		);

		// ---------------------------------------------------------------
		// Handle input change -- the hot path, must be fast
		// ---------------------------------------------------------------
		const handleInputChange = useCallback(
			(e: React.ChangeEvent<HTMLTextAreaElement>) => {
				const value = e.target.value;
				messageInputRef.current = value;

				// Update slash query ref synchronously (cheap)
				slashQueryRef.current = parseSlashInput(value);

				if (dictation.isActive) {
					e.target.style.height = "36px";
				} else {
					resizeTextarea();
				}

				// Notify draft (debounced)
				notifyDraft(value);

				// Defer all React state updates
				scheduleDeferredUpdates(value);
			},
			[
				dictation.isActive,
				resizeTextarea,
				notifyDraft,
				scheduleDeferredUpdates,
			],
		);

		// ---------------------------------------------------------------
		// Handle send
		// ---------------------------------------------------------------
		const handleSend = useCallback(() => {
			const text = messageInputRef.current.trim();
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
			setTextareaValue("");
			setFileAttachments([]);
			setPendingUploads([]);
			setCanSendState(false);
		}, [pendingUploads, fileAttachments, dictation, onSend, setTextareaValue]);

		// ---------------------------------------------------------------
		// Handle key down
		// ---------------------------------------------------------------
		const handleInputKeyDown = useCallback(
			(e: React.KeyboardEvent<HTMLTextAreaElement>) => {
				const sq = slashQueryRef.current;
				if (showSlashPopup && sq.isSlash && !sq.args) {
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
			[showSlashPopup, showFileMentionPopup, handleSend],
		);

		// ---------------------------------------------------------------
		// Handle file upload
		// ---------------------------------------------------------------
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

		// ---------------------------------------------------------------
		// Handle paste
		// ---------------------------------------------------------------
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

		// ---------------------------------------------------------------
		// Handle slash command select
		// ---------------------------------------------------------------
		const handleSlashCommandSelect = useCallback(
			(command: SlashCommand) => {
				setTextareaValue(`/${command.name} `);
				slashQueryRef.current = parseSlashInput(`/${command.name} `);
				setShowSlashPopup(false);
				chatInputRef.current?.focus();
			},
			[setTextareaValue],
		);

		// ---------------------------------------------------------------
		// Handle file mention select
		// ---------------------------------------------------------------
		const handleFileMentionSelect = useCallback(
			(file: FileAttachment) => {
				const newInput = messageInputRef.current.replace(/@[^\s]*$/, "");
				setTextareaValue(newInput);
				setFileAttachments((prev) => [...prev, file]);
				setShowFileMentionPopup(false);
				chatInputRef.current?.focus();
			},
			[setTextareaValue],
		);

		const canSend =
			canSendState || pendingUploads.length > 0 || fileAttachments.length > 0;

		const translations = useMemo(
			() => ({
				inputPlaceholder: t('chat.placeholder'),
			}),
			[t],
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
							query={slashQueryRef.current.command}
							isOpen={
								showSlashPopup &&
								slashQueryRef.current.isSlash &&
								!slashQueryRef.current.args
							}
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
								value={messageInputRef.current}
								liveTranscript={dictation.liveTranscript}
								placeholder={t('chat.speakNow')}
								vadProgress={dictation.vadProgress}
								autoSend={dictation.autoSendEnabled}
								onAutoSendChange={dictation.setAutoSendEnabled}
								onStop={() => {
									dictation.cancel();
									requestAnimationFrame(() => {
										setTextareaValue(messageInputRef.current);
									});
								}}
								onChange={handleInputChange}
								onKeyDown={handleInputKeyDown}
								onPaste={handlePaste}
								onBlur={() => {
									setTimeout(() => setShowSlashPopup(false), 150);
								}}
								onFocus={(e) => {
									if (messageInputRef.current.startsWith("/")) {
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
								placeholder={placeholder ?? translations.inputPlaceholder}
								defaultValue={initialValue}
								onChange={handleInputChange}
								onKeyDown={handleInputKeyDown}
								onPaste={handlePaste}
								onBlur={() => {
									setTimeout(() => setShowSlashPopup(false), 150);
								}}
								onFocus={(e) => {
									if (messageInputRef.current.startsWith("/")) {
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
