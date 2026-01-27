// File type detection and categorization utilities

export type FileCategory =
	| "code"
	| "markdown"
	| "image"
	| "video"
	| "pdf"
	| "csv"
	| "json"
	| "yaml"
	| "xml"
	| "typst"
	| "text"
	| "binary"
	| "unknown";

export interface FileTypeInfo {
	extension: string;
	category: FileCategory;
	language?: string;
	mimeType?: string;
	icon?: string;
}

// Extension to language mapping for syntax highlighting
const extensionToLanguage: Record<string, string> = {
	// JavaScript/TypeScript
	js: "javascript",
	jsx: "jsx",
	ts: "typescript",
	tsx: "tsx",
	mjs: "javascript",
	cjs: "javascript",

	// Web
	html: "html",
	htm: "html",
	css: "css",
	scss: "scss",
	sass: "sass",
	less: "less",
	vue: "vue",
	svelte: "svelte",

	// Data formats
	json: "json",
	yaml: "yaml",
	yml: "yaml",
	xml: "xml",
	toml: "toml",
	csv: "csv",
	ini: "ini",
	conf: "ini",
	cfg: "ini",
	properties: "properties",

	// Config files
	env: "bash",
	envrc: "bash",
	sh: "bash",
	bash: "bash",
	zsh: "bash",
	fish: "bash",
	ps1: "powershell",
	bat: "batch",
	cmd: "batch",

	// Programming languages
	py: "python",
	rb: "ruby",
	go: "go",
	rs: "rust",
	java: "java",
	kt: "kotlin",
	scala: "scala",
	c: "c",
	cpp: "cpp",
	h: "c",
	hpp: "cpp",
	cs: "csharp",
	swift: "swift",
	php: "php",
	pl: "perl",
	lua: "lua",
	r: "r",
	dart: "dart",
	elm: "elm",
	erl: "erlang",
	ex: "elixir",
	exs: "elixir",
	clj: "clojure",
	hs: "haskell",
	ml: "ocaml",
	fs: "fsharp",
	nim: "nim",
	zig: "zig",
	v: "v",
	cr: "crystal",

	// Markup/Document
	md: "markdown",
	mdx: "markdown",
	markdown: "markdown",
	rst: "restructuredtext",
	tex: "latex",
	latex: "latex",
	typ: "typst",
	j2: "jinja2",
	jinja: "jinja2",
	jinja2: "jinja2",
	njk: "jinja2",
	nunjucks: "jinja2",
	tmpl: "jinja2",
	tpl: "jinja2",
	hbs: "handlebars",
	handlebars: "handlebars",
	mustache: "mustache",

	// Database
	sql: "sql",
	graphql: "graphql",
	gql: "graphql",

	// DevOps
	dockerfile: "dockerfile",
	docker: "dockerfile",
	containerfile: "dockerfile",
	makefile: "makefile",
	cmake: "cmake",
	nginx: "nginx",
	tf: "hcl",
	hcl: "hcl",

	// Other
	diff: "diff",
	patch: "diff",
	proto: "protobuf",
	asm: "nasm",
	example: "text",
	txt: "text",
	text: "text",
	log: "text",
};

// Image extensions
const imageExtensions = new Set([
	"png",
	"jpg",
	"jpeg",
	"gif",
	"webp",
	"svg",
	"ico",
	"bmp",
	"tiff",
	"tif",
	"avif",
	"heic",
	"heif",
]);

// Video extensions
const videoExtensions = new Set([
	"mp4",
	"webm",
	"ogg",
	"ogv",
	"mov",
	"avi",
	"mkv",
	"m4v",
]);

// Binary file extensions (not displayable as text)
const binaryExtensions = new Set([
	"doc",
	"docx",
	"xls",
	"xlsx",
	"ppt",
	"pptx",
	"zip",
	"tar",
	"gz",
	"bz2",
	"7z",
	"rar",
	"exe",
	"dll",
	"so",
	"dylib",
	"wasm",
	"bin",
	"dat",
	"db",
	"sqlite",
	"mp3",
	"wav",
	"flac",
	"ttf",
	"otf",
	"woff",
	"woff2",
	"eot",
]);

export function getFileExtension(filename: string): string {
	// Handle special files without extensions
	const lowerName = filename.toLowerCase();

	// Special case files
	const specialFiles: Record<string, string> = {
		dockerfile: "dockerfile",
		containerfile: "dockerfile",
		makefile: "makefile",
		cmakelists: "cmake",
		".gitignore": "gitignore",
		".gitattributes": "gitignore",
		".gitmodules": "gitignore",
		".dockerignore": "dockerignore",
		".env": "env",
		".env.local": "env",
		".env.example": "env",
		".envrc": "envrc",
		".editorconfig": "ini",
		".npmrc": "ini",
		".yarnrc": "ini",
		".pnpmrc": "ini",
		".prettierignore": "gitignore",
		".eslintignore": "gitignore",
		".prettierrc": "json",
		".eslintrc": "json",
		".babelrc": "json",
		".stylelintrc": "json",
		justfile: "makefile",
		"justfile.js": "javascript",
	};

	for (const [special, lang] of Object.entries(specialFiles)) {
		if (lowerName === special || lowerName.endsWith(`/${special}`)) {
			return lang;
		}
	}

	const parts = filename.split(".");
	if (parts.length > 1) {
		return parts[parts.length - 1].toLowerCase();
	}
	return "";
}

export function getFileTypeInfo(filename: string): FileTypeInfo {
	const ext = getFileExtension(filename);
	const lowerExt = ext.toLowerCase();

	// Check for images
	if (imageExtensions.has(lowerExt)) {
		return {
			extension: ext,
			category: "image",
			mimeType: `image/${lowerExt === "svg" ? "svg+xml" : lowerExt}`,
		};
	}

	// Check for videos
	if (videoExtensions.has(lowerExt)) {
		const mimeTypes: Record<string, string> = {
			mp4: "video/mp4",
			webm: "video/webm",
			ogg: "video/ogg",
			ogv: "video/ogg",
			mov: "video/quicktime",
			avi: "video/x-msvideo",
			mkv: "video/x-matroska",
			m4v: "video/x-m4v",
		};
		return {
			extension: ext,
			category: "video",
			mimeType: mimeTypes[lowerExt] || "video/mp4",
		};
	}

	// Check for PDF
	if (lowerExt === "pdf") {
		return {
			extension: ext,
			category: "pdf",
			mimeType: "application/pdf",
		};
	}

	// Check for CSV
	if (lowerExt === "csv") {
		return {
			extension: ext,
			category: "csv",
			language: "csv",
		};
	}

	// Check for markdown
	if (["md", "mdx", "markdown"].includes(lowerExt)) {
		return {
			extension: ext,
			category: "markdown",
			language: "markdown",
		};
	}

	// Check for JSON
	if (lowerExt === "json") {
		return {
			extension: ext,
			category: "json",
			language: "json",
		};
	}

	// Check for YAML
	if (["yaml", "yml"].includes(lowerExt)) {
		return {
			extension: ext,
			category: "yaml",
			language: "yaml",
		};
	}

	// Check for XML
	if (lowerExt === "xml") {
		return {
			extension: ext,
			category: "xml",
			language: "xml",
		};
	}

	// Check for Typst
	if (lowerExt === "typ") {
		return {
			extension: ext,
			category: "typst",
			language: "typst",
		};
	}

	// Check for binary
	if (binaryExtensions.has(lowerExt)) {
		return {
			extension: ext,
			category: "binary",
		};
	}

	// Check for code with known language
	const language = extensionToLanguage[lowerExt];
	if (language) {
		return {
			extension: ext,
			category: "code",
			language,
		};
	}

	// Check for text files (no extension or common text extensions)
	if (ext === "" || ["txt", "log", "text"].includes(lowerExt)) {
		return {
			extension: ext,
			category: "text",
			language: "text",
		};
	}

	// Unknown extension - treat as text
	return {
		extension: ext,
		category: "unknown",
		language: "text",
	};
}

export function isTextFile(filename: string): boolean {
	const info = getFileTypeInfo(filename);
	return !["binary", "image", "video", "pdf"].includes(info.category);
}

export function isBinaryFile(filename: string): boolean {
	const info = getFileTypeInfo(filename);
	return (
		info.category === "binary" ||
		info.category === "image" ||
		info.category === "video" ||
		info.category === "pdf"
	);
}

export function getSyntaxLanguage(filename: string): string {
	const info = getFileTypeInfo(filename);
	return info.language || "text";
}

/**
 * Extract @file references from text content, excluding those inside code blocks.
 * Returns unique file paths without the @ prefix.
 */
type FileReferenceDetail = {
	filePath: string;
	label: string;
	raw: string;
};

function stripTrailingPunctuation(value: string) {
	return value.replace(/[),.;:!?\]}]+$/g, "");
}

function splitReferenceSuffix(value: string) {
	const trimmed = stripTrailingPunctuation(value);
	const match = /[†‡✝✟]/.exec(trimmed);
	if (!match || match.index === undefined) {
		return { filePath: trimmed, suffix: "" };
	}
	return {
		filePath: trimmed.slice(0, match.index),
		suffix: trimmed.slice(match.index),
	};
}

export function extractFileReferenceDetails(
	content: string,
): FileReferenceDetail[] {
	// Remove code blocks (both fenced ``` and inline `)
	// Fenced code blocks: ```...``` or ~~~...~~~
	const withoutFencedBlocks = content.replace(
		/```[\s\S]*?```|~~~[\s\S]*?~~~/g,
		"",
	);
	// Inline code: `...`
	const withoutInlineCode = withoutFencedBlocks.replace(/`[^`\n]+`/g, "");

	const fileRefPattern = /@([^\s@`"'<>()[\]{}]+)/g;
	const details: FileReferenceDetail[] = [];
	const seen = new Set<string>();

	let match: RegExpExecArray | null;
	while ((match = fileRefPattern.exec(withoutInlineCode)) !== null) {
		const raw = match[1];
		const { filePath, suffix } = splitReferenceSuffix(raw);
		if (!/\.[a-zA-Z0-9]+$/.test(filePath)) {
			continue;
		}
		const fileName = filePath.split("/").pop() || filePath;
		const label = suffix ? `${fileName} ${suffix}` : fileName;
		const key = `${filePath}|${suffix}`;
		if (seen.has(key)) continue;
		seen.add(key);
		details.push({ filePath, label, raw });
	}

	return details;
}

export function extractFileReferences(content: string): string[] {
	return extractFileReferenceDetails(content).map((detail) => detail.filePath);
}
