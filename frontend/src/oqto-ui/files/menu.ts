/**
 * What the Files resource menu offers, as data. The pane renders this and
 * the host runs it; nothing here touches the DOM, so which items appear for
 * a directory, a file, a multi-selection or empty space is unit-testable.
 */

import type {
	ActionOffer,
	ResourceSubject,
} from "../platform/actions-contract";
import { entryKind } from "./kinds";
import type { FilesState } from "./navigator";

/** A host command the Files pane implements itself. */
export type FileCommand =
	| "open"
	| "rename"
	| "createFolder"
	| "copy"
	| "cut"
	| "paste"
	| "delete";

export interface MenuItem {
	/** A host command, or an Action id the Broker will run. */
	readonly command: FileCommand | null;
	readonly actionId: string | null;
	/** i18n key for a host command; contributed Actions carry their own title. */
	readonly labelKey: string | null;
	readonly title: string | null;
	readonly danger?: boolean;
}

export interface MenuGroup {
	readonly id: "open" | "edit" | "clipboard" | "danger" | "contributed";
	readonly items: readonly MenuItem[];
}

function host(
	command: FileCommand,
	labelKey: string,
	danger?: boolean,
): MenuItem {
	return { command, actionId: null, labelKey, title: null, danger };
}

/** Media type from the entry name, coarse enough for Action matching. */
export function mediaTypeOf(name: string, directory: boolean): string {
	if (directory) return "inode/directory";
	const kind = entryKind(name, directory);
	if (kind === "image") return "image/*";
	if (kind === "media") return "video/*";
	if (kind === "archive") return "application/octet-stream";
	if (kind === "binary") return "application/octet-stream";
	return "text/plain";
}

/**
 * The subjects a menu acts on: the selection when the target is part of it,
 * otherwise the target alone. Empty space acts on the directory itself.
 */
export function menuSubjects(
	state: FilesState,
	workspacePath: string,
	targetPath: string | null,
): readonly ResourceSubject[] {
	const paths =
		targetPath === null
			? []
			: state.selection.includes(targetPath)
				? state.selection
				: [targetPath];
	const listing = state.listings[state.cwd];
	const entries = listing?.status === "ready" ? listing.entries : [];
	return paths.map((path) => {
		const entry = entries.find((candidate) => candidate.path === path);
		const name = entry?.name ?? (path.split("/").pop() || path);
		return {
			kind: "workspace-file" as const,
			workspacePath,
			reference: path,
			mediaType: mediaTypeOf(name, entry?.directory ?? false),
			label: name,
		};
	});
}

/**
 * The menu for a right-click: host commands first, contributed Actions last
 * and clearly their own group. Empty space offers only what applies to the
 * directory.
 */
export function fileMenu(
	state: FilesState,
	targetPath: string | null,
	offers: readonly ActionOffer[],
): readonly MenuGroup[] {
	const groups: MenuGroup[] = [];
	if (targetPath !== null) {
		groups.push({ id: "open", items: [host("open", "oqtoUi.files.open")] });
		groups.push({
			id: "edit",
			items: [host("rename", "oqtoUi.files.rename")],
		});
	}
	const clipboard: MenuItem[] = [];
	if (targetPath !== null) {
		clipboard.push(host("copy", "oqtoUi.files.copy"));
		clipboard.push(host("cut", "oqtoUi.files.cut"));
	}
	if (state.clipboard !== null)
		clipboard.push(host("paste", "oqtoUi.files.paste"));
	clipboard.push(host("createFolder", "oqtoUi.files.newFolder"));
	groups.push({ id: "clipboard", items: clipboard });
	if (offers.length > 0) {
		groups.push({
			id: "contributed",
			items: offers.map((offer) => ({
				command: null,
				actionId: offer.id,
				labelKey: null,
				title: offer.title,
			})),
		});
	}
	if (targetPath !== null) {
		groups.push({
			id: "danger",
			items: [host("delete", "oqtoUi.files.delete", true)],
		});
	}
	return groups;
}
