import { ToolCallCard } from "@/components/chat/tool-call-card";
import { MarkdownRenderer } from "@/components/data-display/markdown-renderer";
import { extractFileReferenceDetails } from "@/lib/file-types";
import type { MessagePart } from "@/lib/message-part";
import { FileCode2 } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { ChatMessage, ChatMessagePart } from "../platform/contracts";

type ToolResultPart = Extract<ChatMessagePart, { type: "tool_result" }>;
type FileRange = { startLine?: number; endLine?: number };

type MessagePartsProps = {
	message: ChatMessage;
	sessionId: string;
	resultByCallId: ReadonlyMap<string, ToolResultPart>;
	knownCallIds: ReadonlySet<string>;
	onOpenFile: (path: string, range?: FileRange) => void;
};

function stringify(value: ToolResultPart["output"]): string {
	if (value === undefined || value === null) return "";
	return typeof value === "string" ? value : JSON.stringify(value, null, 2);
}

function toolPart(
	part: Extract<ChatMessagePart, { type: "tool_call" }>,
	result: ToolResultPart | undefined,
	sessionId: string,
	messageId: string,
): MessagePart {
	const rawInput = part.input;
	const input =
		rawInput === undefined
			? undefined
			: typeof rawInput === "object" &&
					rawInput !== null &&
					!Array.isArray(rawInput)
				? rawInput
				: { value: rawInput };
	const status = result
		? result.isError
			? "error"
			: "completed"
		: part.status === "success"
			? "completed"
			: part.status;
	return {
		id: part.id,
		sessionID: sessionId,
		messageID: messageId,
		type: "tool",
		tool: result?.name ?? part.name,
		callID: part.toolCallId,
		state: {
			status,
			input,
			output: stringify(result?.output),
			title: result?.name ?? part.name,
		},
	};
}

function orphanResultPart(
	part: ToolResultPart,
	sessionId: string,
	messageId: string,
): MessagePart {
	return {
		id: part.id,
		sessionID: sessionId,
		messageID: messageId,
		type: "tool",
		tool: part.name ?? "tool",
		callID: part.toolCallId,
		state: {
			status: part.isError ? "error" : "completed",
			output: stringify(part.output),
			title: part.name,
		},
	};
}

export function MessageParts({
	message,
	sessionId,
	resultByCallId,
	knownCallIds,
	onOpenFile,
}: MessagePartsProps) {
	const { t } = useTranslation();
	const parts = message.parts ?? [];
	if (parts.length === 0) {
		return (
			<MarkdownRenderer
				content={message.content}
				onFileReferenceOpen={(reference) =>
					onOpenFile(reference.filePath, {
						startLine: reference.startLine,
						endLine: reference.endLine,
					})
				}
			/>
		);
	}
	return (
		<div className="wb-message-parts">
			{parts.map((part) => {
				if (part.type === "text") {
					const references = extractFileReferenceDetails(part.text).filter(
						(reference) => part.text.includes(`@${reference.raw}`),
					);
					return (
						<div className="wb-message-part" key={part.id}>
							<MarkdownRenderer
								content={part.text}
								onFileReferenceOpen={(reference) =>
									onOpenFile(reference.filePath, {
										startLine: reference.startLine,
										endLine: reference.endLine,
									})
								}
							/>
							{references.length > 0 ? (
								<div className="wb-file-refs">
									{references.map((reference) => (
										<button
											key={`${reference.filePath}:${reference.startLine ?? ""}`}
											type="button"
											aria-label={t("oqtoUi.chat.openFile", {
												path: reference.label,
											})}
											onClick={() =>
												onOpenFile(reference.filePath, {
													startLine: reference.startLine,
													endLine: reference.endLine,
												})
											}
										>
											<FileCode2 aria-hidden="true" />
											{reference.label}
										</button>
									))}
								</div>
							) : null}
						</div>
					);
				}
				if (part.type === "thinking") {
					return (
						<details className="wb-thinking-part" key={part.id}>
							<summary>{t("oqtoUi.chat.thinking")}</summary>
							<MarkdownRenderer content={part.text} />
						</details>
					);
				}
				if (part.type === "tool_call") {
					return (
						<ToolCallCard
							key={part.id}
							hideTodoTools
							defaultCollapsed
							part={toolPart(
								part,
								resultByCallId.get(part.toolCallId),
								sessionId,
								message.id,
							)}
						/>
					);
				}
				if (part.type === "tool_result") {
					return knownCallIds.has(part.toolCallId) ? null : (
						<ToolCallCard
							key={part.id}
							hideTodoTools
							defaultCollapsed
							part={orphanResultPart(part, sessionId, message.id)}
						/>
					);
				}
				return (
					<button
						className="wb-structured-file-ref"
						key={part.id}
						type="button"
						onClick={() => onOpenFile(part.uri, part.range)}
					>
						<FileCode2 aria-hidden="true" />
						{part.label ?? part.uri}
					</button>
				);
			})}
		</div>
	);
}
