/**
 * Which viewer opens a file, decided once from its kind. The registry is
 * the single seam between "what a file is" and "how it is shown", so a new
 * format is one entry here plus one component — never a branch scattered
 * through the pane.
 */

import { type EntryKind, entryKind } from "../kinds";

export type ViewerId = "text" | "image" | "media" | "pdf" | "unsupported";

/** Formats whose kind is too coarse to pick the viewer on its own. */
const BY_EXTENSION: { readonly [extension: string]: ViewerId } = {
	pdf: "pdf",
};

const BY_KIND: { readonly [kind in EntryKind]: ViewerId } = {
	folder: "unsupported",
	code: "text",
	markup: "text",
	data: "text",
	document: "text",
	image: "image",
	media: "media",
	archive: "unsupported",
	binary: "unsupported",
	file: "unsupported",
};

export function viewerFor(name: string, directory: boolean): ViewerId {
	if (directory) return "unsupported";
	const extension = name.toLowerCase().split(".").pop() ?? "";
	return BY_EXTENSION[extension] ?? BY_KIND[entryKind(name, directory)];
}

/** Media that a host <video>/<audio> element can play in place. */
export function mediaElement(name: string): "video" | "audio" {
	const extension = name.toLowerCase().split(".").pop() ?? "";
	return ["mp3", "wav", "flac", "ogg", "m4a", "opus"].includes(extension)
		? "audio"
		: "video";
}
