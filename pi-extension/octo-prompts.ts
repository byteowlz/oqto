/**
 * Octo Prompts Extension for Pi
 *
 * Auto-loads USER.md and PERSONALITY.md from the current working directory
 * and appends them to the system prompt.
 */

import * as fs from "node:fs";
import * as path from "node:path";
import type { ExtensionAPI } from "@mariozechner/pi-coding-agent";

function readTextFile(filePath: string): string | null {
	try {
		if (!fs.existsSync(filePath)) return null;
		const content = fs.readFileSync(filePath, "utf8").trim();
		return content.length > 0 ? content : null;
	} catch {
		return null;
	}
}

export default function octoPromptsExtension(pi: ExtensionAPI) {
	let userContent: string | null = null;
	let personalityContent: string | null = null;

	pi.on("session_start", async (_event, ctx) => {
		userContent = readTextFile(path.join(ctx.cwd, "USER.md"));
		personalityContent = readTextFile(path.join(ctx.cwd, "PERSONALITY.md"));
	});

	pi.on("before_agent_start", async () => {
		if (!userContent && !personalityContent) return;

		const sections: string[] = [];
		if (userContent) {
			sections.push(`## User\n\n${userContent}`);
		}
		if (personalityContent) {
			sections.push(`## Personality\n\n${personalityContent}`);
		}

		return {
			systemPromptAppend: `\n${sections.join("\n\n")}\n`,
		};
	});
}
