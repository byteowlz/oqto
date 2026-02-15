import type { MessagePart } from "@/lib/agent-client";

export function getMessageText(parts: MessagePart[]): string {
	return parts
		.filter(
			(part): part is MessagePart & { type: "text"; text: string } =>
				part.type === "text" && typeof part.text === "string",
		)
		.map((part) => part.text)
		.join("\n\n");
}
