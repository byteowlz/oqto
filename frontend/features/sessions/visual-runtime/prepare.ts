import {
	KNOWN_REMOTE_ASSET_PATTERNS,
	LOCAL_ASSET_PATHS,
	buildVisualRuntimeBootstrap,
	cspForMode,
} from "./policy";
import type {
	PrepareVisualRuntimeInput,
	PrepareVisualRuntimeResult,
	VisualRuntimeCapability,
} from "./types";

const REMOTE_PROTOCOL_RE = /^https?:\/\//i;
const JAVASCRIPT_PROTOCOL_RE = /^javascript:/i;
const INLINE_REMOTE_IMPORT_RE =
	/(?:import\s+[^\n;]+\s+from\s+["']https?:\/\/|https?:\/\/esm\.sh|https?:\/\/cdn\.jsdelivr\.net|https?:\/\/unpkg\.com)/i;

function resolveKnownRemoteAsset(url: string): {
	capability: VisualRuntimeCapability;
	localPath: string;
} | null {
	for (const entry of KNOWN_REMOTE_ASSET_PATTERNS) {
		if (entry.pattern.test(url)) {
			return {
				capability: entry.capability,
				localPath: LOCAL_ASSET_PATHS[entry.capability],
			};
		}
	}
	return null;
}

function ensureDocument(html: string): Document {
	const parser = new DOMParser();
	return parser.parseFromString(html, "text/html");
}

function ensureHead(doc: Document): HTMLHeadElement {
	if (doc.head) return doc.head;
	const head = doc.createElement("head");
	doc.documentElement.prepend(head);
	return head;
}

function stripEventHandlerAttributes(node: Element): number {
	let removed = 0;
	for (const attr of Array.from(node.attributes)) {
		if (attr.name.toLowerCase().startsWith("on")) {
			node.removeAttribute(attr.name);
			removed += 1;
		}
	}
	return removed;
}

function generateNonce(): string {
	if (
		typeof crypto !== "undefined" &&
		typeof crypto.getRandomValues === "function"
	) {
		const bytes = new Uint8Array(16);
		crypto.getRandomValues(bytes);
		return Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join("");
	}
	return `${Date.now().toString(16)}${Math.random().toString(16).slice(2)}`;
}

export function prepareVisualRuntimeDocument({
	html,
	mode,
}: PrepareVisualRuntimeInput): PrepareVisualRuntimeResult {
	const doc = ensureDocument(html);
	const diagnostics: PrepareVisualRuntimeResult["diagnostics"] = [];
	const requires = new Set<VisualRuntimeCapability>();
	const nonce = generateNonce();

	// Harden event-handler attributes and javascript: URLs.
	for (const element of doc.querySelectorAll("*")) {
		const removedHandlers = stripEventHandlerAttributes(element);
		if (removedHandlers > 0) {
			diagnostics.push({
				level: "warn",
				message: `Removed ${removedHandlers} inline event handler attribute(s).`,
			});
		}
		for (const attr of ["href", "src"]) {
			const value = element.getAttribute(attr);
			if (value && JAVASCRIPT_PROTOCOL_RE.test(value.trim())) {
				element.removeAttribute(attr);
				diagnostics.push({
					level: "error",
					message: `Blocked javascript: URL in ${attr}.`,
				});
			}
		}
	}

	// Rewrite/block remote scripts.
	for (const script of doc.querySelectorAll("script")) {
		const src = script.getAttribute("src")?.trim();
		if (src && REMOTE_PROTOCOL_RE.test(src)) {
			const known = resolveKnownRemoteAsset(src);
			if (known && mode !== "online_flexible") {
				script.setAttribute("src", known.localPath);
				script.removeAttribute("type");
				requires.add(known.capability);
				diagnostics.push({
					level: "info",
					message: `Rewrote remote asset ${src} -> ${known.localPath}`,
				});
				continue;
			}
			if (mode === "offline_strict") {
				script.remove();
				diagnostics.push({
					level: "error",
					message: `Blocked remote script in offline_strict mode: ${src}`,
				});
			}
			continue;
		}

		const inlineCode = script.textContent ?? "";
		if (mode === "offline_strict" && INLINE_REMOTE_IMPORT_RE.test(inlineCode)) {
			script.remove();
			diagnostics.push({
				level: "error",
				message:
					"Blocked inline script with remote import/fetch reference in offline_strict mode.",
			});
		}
	}

	// Block remote styles/fonts in strict mode.
	for (const link of doc.querySelectorAll("link")) {
		const href = link.getAttribute("href")?.trim();
		if (!href || !REMOTE_PROTOCOL_RE.test(href)) continue;
		if (mode === "offline_strict") {
			link.remove();
			diagnostics.push({
				level: "error",
				message: `Blocked remote stylesheet/font in offline_strict mode: ${href}`,
			});
		} else if (mode === "offline_prefer") {
			diagnostics.push({
				level: "warn",
				message: `Remote stylesheet/font allowed in offline_prefer mode: ${href}`,
			});
		}
	}

	// Block remote images in strict mode.
	for (const img of doc.querySelectorAll("img")) {
		const src = img.getAttribute("src")?.trim();
		if (!src || !REMOTE_PROTOCOL_RE.test(src)) continue;
		if (mode === "offline_strict") {
			img.removeAttribute("src");
			img.setAttribute("alt", "[remote image blocked in offline_strict]");
			diagnostics.push({
				level: "warn",
				message: `Blocked remote image in offline_strict mode: ${src}`,
			});
		}
	}

	const head = ensureHead(doc);

	// Deduplicate existing CSP meta and inject the active policy.
	for (const meta of head.querySelectorAll("meta[http-equiv]")) {
		if (
			meta.getAttribute("http-equiv")?.toLowerCase() ===
			"content-security-policy"
		) {
			meta.remove();
		}
	}
	const cspMeta = doc.createElement("meta");
	cspMeta.setAttribute("http-equiv", "Content-Security-Policy");
	cspMeta.setAttribute("content", cspForMode(mode, nonce));
	head.prepend(cspMeta);

	// Infer capabilities from content markers + already-local script tags.
	if (doc.querySelector(".mermaid")) {
		requires.add("mermaid");
		requires.add("layout_elk");
	}
	if (
		doc.querySelector("canvas[data-chartjs], canvas.chartjs") ||
		doc.body.innerHTML.includes("Chart(")
	) {
		requires.add("chartjs");
	}
	if (doc.body.innerHTML.includes("morphdom(")) {
		requires.add("morphdom");
	}

	for (const script of doc.querySelectorAll("script[src]")) {
		const src = script.getAttribute("src") ?? "";
		for (const [capability, path] of Object.entries(LOCAL_ASSET_PATHS)) {
			if (src.includes(path)) {
				requires.add(capability as VisualRuntimeCapability);
			}
		}
	}

	// Inject local runtime assets for requested capabilities.
	for (const capability of requires) {
		const localPath = LOCAL_ASSET_PATHS[capability];
		const exists = doc.querySelector(`script[src="${localPath}"]`);
		if (exists) continue;
		if (capability === "layout_elk") {
			const moduleScript = doc.createElement("script");
			moduleScript.setAttribute("type", "module");
			moduleScript.textContent = `import * as elk from '${localPath}'; window.mermaidLayoutELK = elk.default ?? elk;`;
			head.append(moduleScript);
			continue;
		}
		const script = doc.createElement("script");
		script.setAttribute("src", localPath);
		head.append(script);
	}

	const bootstrap = doc.createElement("template");
	bootstrap.innerHTML = buildVisualRuntimeBootstrap([...requires]);
	head.append(...bootstrap.content.childNodes);

	for (const script of doc.querySelectorAll("script")) {
		script.setAttribute("nonce", nonce);
	}
	for (const style of doc.querySelectorAll("style")) {
		style.setAttribute("nonce", nonce);
	}
	diagnostics.push({
		level: "info",
		message: "Applied CSP nonce to inline scripts and styles.",
	});

	return {
		html: `<!doctype html>\n${doc.documentElement.outerHTML}`,
		requires: [...requires],
		diagnostics,
	};
}
