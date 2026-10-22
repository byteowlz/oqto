import type { OqtoUiPlatform } from "./contracts";
import type { FileChange } from "./files-contract";
interface FlashWorkspace {
	list(
		path: string,
	): Promise<
		readonly {
			path: string;
			name: string;
			kind: string;
			size: number;
			mtimeMs: number;
		}[]
	>;
	read(path: string): Promise<string>;
	subscribe(listener: (change: FileChange) => void): () => void;
}

type FlashHosts = Pick<
	OqtoUiPlatform,
	"files" | "issues" | "git" | "terminal" | "actions"
>;

const ROOT = "/workspace";

function relative(path: string): string {
	const value =
		path === ROOT
			? ""
			: path.startsWith(`${ROOT}/`)
				? path.slice(ROOT.length + 1)
				: path;
	if (
		value.startsWith("/") ||
		value.includes("\\") ||
		value.split("/").some((part) => part === ".." || part === ".")
	) {
		throw new Error("Flash files require a path inside /workspace");
	}
	return value.replace(/\/+$/, "");
}

function workspace(path: string): void {
	if (path !== ROOT && path !== `${ROOT}/`) {
		throw new Error("Flash workspace is /workspace");
	}
}

function unavailable(capability: string): () => Promise<never> {
	return async () => {
		throw new Error(`${capability} is unavailable in browser Flash`);
	};
}

/** Browser-only ports. No server fallback or invented repository/PTY state. */
export function createFlashHosts(worker: FlashWorkspace): FlashHosts {
	const readOnly = unavailable("File mutation (read-only workspace)");
	const gitUnavailable = unavailable("Git");
	return {
		files: {
			async list(workspacePath, path, includeHidden = false) {
				workspace(workspacePath);
				const directory = relative(path);
				const entries = await worker.list(
					directory ? `${ROOT}/${directory}` : ROOT,
				);
				return entries
					.filter((entry) => includeHidden || !entry.name.startsWith("."))
					.map((entry) => ({
						path: relative(entry.path),
						name: entry.name,
						directory: entry.kind === "directory",
						symlink: entry.kind === "symlink",
						size: entry.size,
						modifiedAt: entry.mtimeMs,
					}));
			},
			async read(workspacePath, path) {
				workspace(workspacePath);
				return worker.read(`${ROOT}/${relative(path)}`);
			},
			watch(workspacePath, onChange) {
				workspace(workspacePath);
				return worker.subscribe(onChange);
			},
			write: readOnly,
			rename: readOnly,
			createDirectory: readOnly,
			remove: readOnly,
			copy: readOnly,
			copyToWorkspace: readOnly,
			move: readOnly,
		},
		issues: { list: unavailable("Issues"), setStatus: unavailable("Issues") },
		git: {
			status: gitUnavailable,
			diff: gitUnavailable,
			log: gitUnavailable,
			stage: gitUnavailable,
			commit: gitUnavailable,
			branches: { list: gitUnavailable, switch: gitUnavailable },
			remote: gitUnavailable,
		},
		terminal: { open: unavailable("PTY terminal") },
		actions: {
			async offers() {
				return [];
			},
			invoke: unavailable("Action Broker"),
		},
	};
}
