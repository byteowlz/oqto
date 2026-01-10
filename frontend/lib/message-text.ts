import type { OpenCodePart } from "@/lib/opencode-client";

export function getMessageText(parts: OpenCodePart[]): string {
	return parts
		.filter(
			(part): part is OpenCodePart & { type: "text"; text: string } =>
				part.type === "text" && typeof part.text === "string",
		)
		.map((part) => part.text)
		.join("\n\n");
}
