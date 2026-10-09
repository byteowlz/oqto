const ANSI_LITERAL_RE = /\\x1b\[[0-9;]*[A-Za-z]/g;

export function stripAnsiSequences(value: string): string {
	let output = "";
	for (let index = 0; index < value.length; index += 1) {
		const code = value.charCodeAt(index);
		if (code === 0x1b || code === 0x9b) {
			index += 1;
			while (index < value.length) {
				const character = value.charCodeAt(index);
				if (character >= 0x40 && character <= 0x7e) break;
				index += 1;
			}
			continue;
		}
		output += value[index];
	}
	return output.replace(ANSI_LITERAL_RE, "");
}

export function dedentMarkdown(text: string): string {
	const lines = text.split("\n");
	let minimumIndent = Number.POSITIVE_INFINITY;
	let inFence = false;
	for (const line of lines) {
		if (/^[ \t]*```/.test(line)) {
			inFence = !inFence;
			continue;
		}
		if (inFence || line.trim().length === 0) continue;
		const match = line.match(/^( +)/);
		minimumIndent = Math.min(minimumIndent, match ? match[1].length : 0);
	}
	if (minimumIndent === 0 || minimumIndent === Number.POSITIVE_INFINITY) {
		return text;
	}
	const prefix = new RegExp(`^[ ]{1,${minimumIndent}}`);
	inFence = false;
	return lines
		.map((line) => {
			if (/^[ \t]*```/.test(line)) {
				inFence = !inFence;
				return line.replace(prefix, "");
			}
			return inFence ? line : line.replace(prefix, "");
		})
		.join("\n");
}
