/**
 * Entry kinds: what an entry *is*, from its name alone. The kind drives the
 * icon and the palette role a row uses, so colour stays semantic rather
 * than per-extension guesswork.
 */

export type EntryKind =
	| "folder"
	| "code"
	| "markup"
	| "data"
	| "document"
	| "image"
	| "media"
	| "archive"
	| "binary"
	| "file";

const BY_EXTENSION: { readonly [extension: string]: EntryKind } = {
	ts: "code",
	tsx: "code",
	js: "code",
	jsx: "code",
	mjs: "code",
	cjs: "code",
	rs: "code",
	go: "code",
	py: "code",
	rb: "code",
	java: "code",
	kt: "code",
	c: "code",
	h: "code",
	cpp: "code",
	hpp: "code",
	cs: "code",
	swift: "code",
	sh: "code",
	bash: "code",
	zsh: "code",
	fish: "code",
	lua: "code",
	sql: "code",
	html: "markup",
	css: "markup",
	scss: "markup",
	svg: "markup",
	xml: "markup",
	json: "data",
	jsonl: "data",
	yaml: "data",
	yml: "data",
	toml: "data",
	ini: "data",
	env: "data",
	lock: "data",
	csv: "data",
	md: "document",
	mdx: "document",
	txt: "document",
	pdf: "document",
	rst: "document",
	adoc: "document",
	png: "image",
	jpg: "image",
	jpeg: "image",
	gif: "image",
	webp: "image",
	avif: "image",
	ico: "image",
	bmp: "image",
	heic: "image",
	mp4: "media",
	webm: "media",
	mkv: "media",
	mov: "media",
	mp3: "media",
	wav: "media",
	flac: "media",
	ogg: "media",
	zip: "archive",
	tar: "archive",
	gz: "archive",
	tgz: "archive",
	bz2: "archive",
	xz: "archive",
	zst: "archive",
	rar: "archive",
	"7z": "archive",
	wasm: "binary",
	so: "binary",
	dylib: "binary",
	dll: "binary",
	exe: "binary",
	bin: "binary",
	o: "binary",
	a: "binary",
};

/** Names that carry meaning regardless of extension. */
const BY_NAME: { readonly [name: string]: EntryKind } = {
	dockerfile: "code",
	justfile: "code",
	makefile: "code",
	license: "document",
	readme: "document",
};

export function entryKind(name: string, directory: boolean): EntryKind {
	if (directory) return "folder";
	const lower = name.toLowerCase();
	const named = BY_NAME[lower] ?? BY_NAME[lower.replace(/\.[^.]+$/, "")];
	if (named) return named;
	const dot = lower.lastIndexOf(".");
	if (dot <= 0) return "file";
	return BY_EXTENSION[lower.slice(dot + 1)] ?? "file";
}

/** True for kinds whose contents a text preview can show. */
export function isTextual(kind: EntryKind): boolean {
	return (
		kind === "code" ||
		kind === "markup" ||
		kind === "data" ||
		kind === "document"
	);
}
