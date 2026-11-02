/** Session image refs are identifiers, never client-selected hosts or paths. */
export function sessionMediaUrl(
	value: unknown,
	variant: "thumb" | "full" = "thumb",
): string | null {
	if (!value || typeof value !== "object") return null;
	const ref = value as Record<string, unknown>;
	if (
		typeof ref.session_id !== "string" ||
		!ref.session_id ||
		ref.session_id.length > 256 ||
		/^(pending-|tmp:)/.test(ref.session_id) ||
		typeof ref.sha256 !== "string" ||
		!/^[0-9a-f]{64}$/.test(ref.sha256)
	)
		return null;
	return `/api/chat-history/${encodeURIComponent(ref.session_id)}/media/${ref.sha256}?variant=${variant}`;
}
export function originalSessionMediaUrl(src: string): string | null {
	try {
		const origin =
			typeof window === "undefined"
				? "http://oqto.invalid"
				: window.location.origin;
		const url = new URL(src, origin);
		if (
			url.origin !== origin ||
			!/^\/api\/chat-history\/[^/]+\/media\/[0-9a-f]{64}$/.test(url.pathname)
		)
			return null;
		url.searchParams.set("variant", "full");
		return `${url.pathname}${url.search}`;
	} catch {
		return null;
	}
}
export function sessionMediaImages(
	output: unknown,
): { src: string; alt: string }[] {
	const images: { src: string; alt: string }[] = [];
	const seen = new Set<string>();
	let visited = 0;
	const visit = (value: unknown, depth: number): void => {
		if (depth > 96 || ++visited > 10000 || !value || typeof value !== "object")
			return;
		if (Array.isArray(value)) {
			for (const item of value) visit(item, depth + 1);
			return;
		}
		const block = value as Record<string, unknown>;
		if (block.type === "image") {
			const src = sessionMediaUrl(block.media_ref);
			if (src && !seen.has(src)) {
				seen.add(src);
				images.push({
					src,
					alt: typeof block.alt === "string" ? block.alt : "Attached content",
				});
			}
			return;
		}
		for (const item of Object.values(block)) visit(item, depth + 1);
	};
	visit(output, 0);
	return images;
}
