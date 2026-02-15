"use client";

import { MarkdownRenderer } from "@/components/data-display";
import { Button } from "@/components/ui/button";
import { Textarea } from "@/components/ui/textarea";
import {
	type MessageWithParts,
	type AgentSession,
	createSession,
	fetchMessages,
	fetchSessions,
	invalidateMessageCache,
	sendMessageAsync,
} from "@/lib/agent-client";
import { cn } from "@/lib/utils";
import { Bot, Loader2, MessageSquare, Plus, Send, User } from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";

const PERSONA_BUILDER_AGENT = "persona-builder";
const SESSION_TITLE_PREFIX = "Persona Builder";

type ChatState = "idle" | "sending";

interface PersonaBuilderChatProps {
	agentBaseUrl: string | null;
	onPersonaCreated?: (personaId: string) => void;
	className?: string;
}

export function PersonaBuilderChat({
	agentBaseUrl,
	onPersonaCreated,
	className,
}: PersonaBuilderChatProps) {
	const [sessions, setSessions] = useState<AgentSession[]>([]);
	const [selectedSessionId, setSelectedSessionId] = useState<string | null>(
		null,
	);
	const [messages, setMessages] = useState<MessageWithParts[]>([]);
	const [messageInput, setMessageInput] = useState("");
	const [chatState, setChatState] = useState<ChatState>("idle");
	const [isLoading, setIsLoading] = useState(true);
	const [showSessionList, setShowSessionList] = useState(false);

	const messagesEndRef = useRef<HTMLDivElement>(null);
	const textareaRef = useRef<HTMLTextAreaElement>(null);

	// Filter sessions to only show persona builder sessions
	const builderSessions = useMemo(() => {
		return sessions.filter((s) => s.title?.startsWith(SESSION_TITLE_PREFIX));
	}, [sessions]);

	// Load sessions on mount
	useEffect(() => {
		if (!agentBaseUrl) return;
		setIsLoading(true);
		fetchSessions(agentBaseUrl)
			.then((allSessions) => {
				setSessions(allSessions);
				// Auto-select the most recent persona builder session
				const builderOnes = allSessions.filter((s) =>
					s.title?.startsWith(SESSION_TITLE_PREFIX),
				);
				if (builderOnes.length > 0) {
					const sorted = [...builderOnes].sort(
						(a, b) => b.time.updated - a.time.updated,
					);
					setSelectedSessionId(sorted[0].id);
				}
			})
			.catch(console.error)
			.finally(() => setIsLoading(false));
	}, [agentBaseUrl]);

	// Load messages when session changes
	const loadMessages = useCallback(async () => {
		if (!agentBaseUrl || !selectedSessionId) {
			setMessages([]);
			return;
		}
		try {
			const data = await fetchMessages(agentBaseUrl, selectedSessionId);
			setMessages(data);
		} catch (err) {
			console.error("Failed to load messages:", err);
		}
	}, [agentBaseUrl, selectedSessionId]);

	useEffect(() => {
		loadMessages();
	}, [loadMessages]);

	// Poll for message updates (replaces legacy SSE subscription)
	useEffect(() => {
		if (!agentBaseUrl || !selectedSessionId) return;
		const interval = setInterval(() => {
			loadMessages();
		}, 3000);
		return () => clearInterval(interval);
	}, [agentBaseUrl, selectedSessionId, loadMessages]);

	// Scroll to bottom when messages change
	useEffect(() => {
		const messageCount = messages.length;
		if (messageCount === 0) return;
		messagesEndRef.current?.scrollIntoView({ behavior: "smooth" });
	}, [messages.length]);

	// Create new session
	const handleNewSession = useCallback(async () => {
		if (!agentBaseUrl) return;
		try {
			const timestamp = new Date().toLocaleString("en-US", {
				month: "short",
				day: "numeric",
				hour: "numeric",
				minute: "2-digit",
			});
			const session = await createSession(
				agentBaseUrl,
				`${SESSION_TITLE_PREFIX} - ${timestamp}`,
			);
			setSessions((prev) => [session, ...prev]);
			setSelectedSessionId(session.id);
			setMessages([]);
			setShowSessionList(false);
		} catch (err) {
			console.error("Failed to create session:", err);
		}
	}, [agentBaseUrl]);

	// Send message
	const handleSend = useCallback(async () => {
		if (!agentBaseUrl || !messageInput.trim()) return;

		// Create session if none selected
		let sessionId = selectedSessionId;
		if (!sessionId) {
			try {
				const timestamp = new Date().toLocaleString("en-US", {
					month: "short",
					day: "numeric",
					hour: "numeric",
					minute: "2-digit",
				});
				const session = await createSession(
					agentBaseUrl,
					`${SESSION_TITLE_PREFIX} - ${timestamp}`,
				);
				setSessions((prev) => [session, ...prev]);
				sessionId = session.id;
				setSelectedSessionId(sessionId);
			} catch (err) {
				console.error("Failed to create session:", err);
				return;
			}
		}

		// Prefix with @persona-builder to invoke the agent
		const messageText = messageInput.trim().startsWith("@")
			? messageInput.trim()
			: `@${PERSONA_BUILDER_AGENT} ${messageInput.trim()}`;

		// Optimistic update
		const optimisticMessage: MessageWithParts = {
			info: {
				id: `temp-${Date.now()}`,
				sessionID: sessionId,
				role: "user",
				time: { created: Date.now() },
			},
			parts: [
				{
					id: `temp-part-${Date.now()}`,
					sessionID: sessionId,
					messageID: `temp-${Date.now()}`,
					type: "text",
					text: messageInput.trim(),
				},
			],
		};

		setMessages((prev) => [...prev, optimisticMessage]);
		setMessageInput("");
		setChatState("sending");

		// Reset textarea height to minimum
		if (textareaRef.current) {
			textareaRef.current.style.height = "36px";
		}

		try {
			await sendMessageAsync(agentBaseUrl, sessionId, messageText);
			invalidateMessageCache(agentBaseUrl, sessionId);
			loadMessages();
		} catch (err) {
			console.error("Failed to send message:", err);
			setChatState("idle");
			setMessages((prev) => prev.filter((m) => !m.info.id.startsWith("temp-")));
		}
	}, [agentBaseUrl, selectedSessionId, messageInput, loadMessages]);

	// Handle Enter key
	const handleKeyDown = useCallback(
		(e: React.KeyboardEvent<HTMLTextAreaElement>) => {
			if (e.key === "Enter" && !e.shiftKey) {
				e.preventDefault();
				handleSend();
			}
		},
		[handleSend],
	);

	// Auto-resize textarea
	const handleTextareaChange = useCallback(
		(e: React.ChangeEvent<HTMLTextAreaElement>) => {
			setMessageInput(e.target.value);
			const textarea = e.target;
			textarea.style.height = "auto";
			// Only expand if there's content, otherwise stay at minimum height
			const newHeight = textarea.value.trim()
				? Math.min(textarea.scrollHeight, 150)
				: 36;
			textarea.style.height = `${newHeight}px`;
		},
		[],
	);

	if (!agentBaseUrl) {
		return (
			<div
				className={cn(
					"flex items-center justify-center h-full text-muted-foreground text-sm",
					className,
				)}
			>
				Connect to start building personas
			</div>
		);
	}

	return (
		<div className={cn("flex flex-col h-full", className)}>
			{/* Header */}
			<div className="flex items-center justify-between p-3 border-b border-border">
				<div className="flex items-center gap-2">
					<Bot className="w-4 h-4 text-primary" />
					<span className="text-sm font-medium">Persona Builder</span>
				</div>
				<div className="flex items-center gap-1">
					{builderSessions.length > 0 && (
						<Button
							type="button"
							variant="ghost"
							size="sm"
							className="h-7 w-7 p-0"
							onClick={() => setShowSessionList(!showSessionList)}
							title="Session history"
						>
							<MessageSquare className="w-3.5 h-3.5" />
						</Button>
					)}
					<Button
						type="button"
						variant="ghost"
						size="sm"
						className="h-7 w-7 p-0"
						onClick={handleNewSession}
						title="New conversation"
					>
						<Plus className="w-3.5 h-3.5" />
					</Button>
				</div>
			</div>

			{/* Session list dropdown */}
			{showSessionList && builderSessions.length > 0 && (
				<div className="border-b border-border bg-muted/50 max-h-32 overflow-y-auto">
					{builderSessions.map((session) => (
						<button
							key={session.id}
							type="button"
							onClick={() => {
								setSelectedSessionId(session.id);
								setShowSessionList(false);
							}}
							className={cn(
								"w-full text-left px-3 py-1.5 text-xs hover:bg-muted transition-colors",
								selectedSessionId === session.id && "bg-muted",
							)}
						>
							<div className="truncate">{session.title}</div>
							<div className="text-muted-foreground text-[10px]">
								{new Date(session.time.updated).toLocaleDateString()}
							</div>
						</button>
					))}
				</div>
			)}

			{/* Messages */}
			<div className="flex-1 overflow-y-auto p-3 space-y-3">
				{isLoading ? (
					<div className="flex items-center justify-center h-full">
						<Loader2 className="w-4 h-4 animate-spin text-muted-foreground" />
					</div>
				) : messages.length === 0 ? (
					<div className="text-center text-muted-foreground text-xs py-8">
						<p className="mb-2">Describe the persona you want to create.</p>
						<p className="text-[10px]">
							Example: "Create a code reviewer that focuses on Python best
							practices"
						</p>
					</div>
				) : (
					messages.map((msg) => {
						const isUser = msg.info.role === "user";
						const textParts = msg.parts.filter((p) => p.type === "text");
						const text = textParts.map((p) => p.text || "").join("\n");

						// For display, strip the @persona-builder prefix from user messages
						const displayText = isUser
							? text.replace(/^@persona-builder\s*/i, "")
							: text;

						if (!displayText.trim()) return null;

						return (
							<div
								key={msg.info.id}
								className={cn(
									"flex gap-2",
									isUser ? "justify-end" : "justify-start",
								)}
							>
								{!isUser && (
									<div className="w-5 h-5 rounded-full bg-primary/10 flex items-center justify-center flex-shrink-0 mt-0.5">
										<Bot className="w-3 h-3 text-primary" />
									</div>
								)}
								<div
									className={cn(
										"max-w-[85%] rounded-lg px-3 py-2 text-xs",
										isUser ? "bg-primary text-primary-foreground" : "bg-muted",
									)}
								>
									{isUser ? (
										<p className="whitespace-pre-wrap">{displayText}</p>
									) : (
										<div className="prose prose-xs dark:prose-invert max-w-none [&_p]:my-1 [&_ul]:my-1 [&_ol]:my-1 [&_pre]:my-1">
											<MarkdownRenderer content={displayText} />
										</div>
									)}
								</div>
								{isUser && (
									<div className="w-5 h-5 rounded-full bg-muted flex items-center justify-center flex-shrink-0 mt-0.5">
										<User className="w-3 h-3" />
									</div>
								)}
							</div>
						);
					})
				)}

				{/* Typing indicator */}
				{chatState === "sending" && (
					<div className="flex gap-2 justify-start">
						<div className="w-5 h-5 rounded-full bg-primary/10 flex items-center justify-center flex-shrink-0">
							<Bot className="w-3 h-3 text-primary" />
						</div>
						<div className="bg-muted rounded-lg px-3 py-2">
							<div className="flex gap-1">
								<span
									className="w-1.5 h-1.5 bg-muted-foreground rounded-full animate-bounce"
									style={{ animationDelay: "0ms" }}
								/>
								<span
									className="w-1.5 h-1.5 bg-muted-foreground rounded-full animate-bounce"
									style={{ animationDelay: "150ms" }}
								/>
								<span
									className="w-1.5 h-1.5 bg-muted-foreground rounded-full animate-bounce"
									style={{ animationDelay: "300ms" }}
								/>
							</div>
						</div>
					</div>
				)}

				<div ref={messagesEndRef} />
			</div>

			{/* Input */}
			<div className="p-3 border-t border-border">
				<div className="flex items-end gap-2">
					<Textarea
						ref={textareaRef}
						value={messageInput}
						onChange={handleTextareaChange}
						onKeyDown={handleKeyDown}
						placeholder="Describe your persona..."
						className="min-h-[36px] max-h-[150px] resize-none text-sm"
						rows={1}
						disabled={chatState === "sending"}
					/>
					<Button
						type="button"
						size="sm"
						className="h-9 w-9 p-0 flex-shrink-0"
						onClick={handleSend}
						disabled={chatState === "sending" || !messageInput.trim()}
					>
						{chatState === "sending" ? (
							<Loader2 className="w-4 h-4 animate-spin" />
						) : (
							<Send className="w-4 h-4" />
						)}
					</Button>
				</div>
			</div>
		</div>
	);
}
