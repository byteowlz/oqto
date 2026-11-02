/** Synthetic deployment for the resource-access exploration. Not service data. */
export type ResourceKind =
	| "account"
	| "workspace"
	| "workdir"
	| "wiki"
	| "memory"
	| "service";
export type GrantMode = "allow" | "ask" | "deny";
export type IconName =
	| "code"
	| "book"
	| "memory"
	| "team"
	| "flask"
	| "globe"
	| "shield"
	| "phone"
	| "key"
	| "terminal"
	| "database";
export interface Operation {
	id: string;
	labelKey: string;
	shortKey: string;
	ownerOnly?: boolean;
}
export interface Resource {
	id: string;
	name: string;
	kind: ResourceKind;
	parent?: string;
	icon?: IconName;
	noteKey: string;
	accessKey: string;
	binding: string;
	/** Example standing grant for service use; never a live authorization. */
	use?: GrantMode;
}
export interface Draft {
	subject: string;
	resource: string;
	operation: string;
	mode: GrantMode;
}

const FILE_OPERATIONS: readonly Operation[] = [
	{ id: "list", labelKey: "List", shortKey: "L" },
	{ id: "read", labelKey: "Read", shortKey: "R" },
	{ id: "modify", labelKey: "Modify", shortKey: "W" },
	{ id: "execute", labelKey: "Execute", shortKey: "X" },
];
const WIKI_OPERATIONS: readonly Operation[] = [
	{ id: "read", labelKey: "Search / read", shortKey: "R" },
	{ id: "contribute", labelKey: "Contribute", shortKey: "W" },
	{ id: "manage", labelKey: "Manage", shortKey: "M", ownerOnly: true },
];
const MEMORY_OPERATIONS: readonly Operation[] = [
	{ id: "read", labelKey: "Retrieve", shortKey: "R" },
	{ id: "contribute", labelKey: "Record", shortKey: "W" },
	{ id: "manage", labelKey: "Delete / manage", shortKey: "M", ownerOnly: true },
];
const SERVICE_OPERATIONS: readonly Operation[] = [
	{ id: "use", labelKey: "Use", shortKey: "U" },
	{ id: "manage", labelKey: "Manage", shortKey: "M", ownerOnly: true },
];
export function operationsFor(kind: ResourceKind): readonly Operation[] {
	if (kind === "workdir") return FILE_OPERATIONS;
	if (kind === "wiki") return WIKI_OPERATIONS;
	if (kind === "memory") return MEMORY_OPERATIONS;
	if (kind === "service") return SERVICE_OPERATIONS;
	return [];
}

const workspace = (
	id: string,
	name: string,
	accessKey: string,
	icon?: IconName,
): Resource => ({
	id,
	name,
	kind: "workspace",
	icon,
	noteKey: accessKey,
	accessKey,
	binding: id === "ws-personal" ? "Local runner" : "Shared placement",
});
const workdir = (
	id: string,
	name: string,
	parent: string,
	icon?: IconName,
): Resource => ({
	id,
	name,
	kind: "workdir",
	parent,
	icon,
	noteKey: "Registered workdir",
	accessKey: "Read / write",
	binding: `runner · /work/${id} (example binding)`,
});
const knowledge = (
	id: string,
	name: string,
	kind: "wiki" | "memory",
	parent: string,
	accessKey: string,
): Resource => ({
	id,
	name,
	kind,
	parent,
	icon: kind === "wiki" ? "book" : "memory",
	noteKey: kind === "wiki" ? "Wiki" : "Memory store",
	accessKey,
	binding: `${kind} provider · ${id} · not a mount`,
});
const service = (
	id: string,
	name: string,
	icon: IconName,
	use: GrantMode,
): Resource => ({
	id,
	name,
	kind: "service",
	icon,
	noteKey: "External connector",
	accessKey: "Session-scoped grant",
	binding: `capability endpoint · ${id}`,
	use,
});

export const resources: readonly Resource[] = [
	{
		id: "account",
		name: "You",
		kind: "account",
		noteKey: "Your resources",
		accessKey: "Account identity",
		binding: "Independent of hosts",
	},
	workspace("ws-personal", "Personal", "Owner"),
	workspace("ws-product", "Product team", "Member", "team"),
	workspace("ws-research", "Research lab", "Member", "flask"),
	workspace("ws-client", "Hanse Logistics", "Restricted client", "shield"),
	// Knowledge: global collections belong to You, scoped ones to a workspace.
	knowledge("wiki-eng", "Engineering", "wiki", "account", "Read / contribute"),
	knowledge("wiki-private", "Private notes", "wiki", "account", "Owner"),
	knowledge("mem-personal", "Personal memory", "memory", "account", "Owner"),
	knowledge(
		"wiki-product",
		"Product handbook",
		"wiki",
		"ws-product",
		"Read / contribute",
	),
	knowledge(
		"mem-product",
		"Team memory",
		"memory",
		"ws-product",
		"Read / record",
	),
	knowledge("wiki-research", "Research methods", "wiki", "ws-research", "Read"),
	knowledge(
		"mem-research",
		"Lab memory",
		"memory",
		"ws-research",
		"Read / record",
	),
	knowledge("wiki-client", "Client confidential", "wiki", "ws-client", "Read"),
	knowledge("mem-client", "Client memory", "memory", "ws-client", "Read"),
	workdir("oqto", "Oqto", "ws-personal", "code"),
	workdir("oqt-cli", "oqt", "ws-personal"),
	workdir("pi-extensions", "Pi extensions", "ws-personal"),
	workdir("dotfiles", "dotfiles", "ws-personal"),
	workdir("notes", "Research notes", "ws-personal"),
	workdir("web-app", "Web app", "ws-product", "globe"),
	workdir("ios-app", "iOS app", "ws-product", "phone"),
	workdir("android-app", "Android app", "ws-product", "phone"),
	workdir("design-system", "Design system", "ws-product"),
	workdir("api-gateway", "API gateway", "ws-product"),
	workdir("rag-eval", "RAG evaluation", "ws-research", "flask"),
	workdir("benchmarks", "Agent benchmarks", "ws-research"),
	workdir("paper", "Paper 2026", "ws-research"),
	workdir("portal", "Customer portal", "ws-client", "globe"),
	workdir("pipeline", "Data pipeline", "ws-client"),
	workdir("audit", "Compliance audit", "ws-client", "shield"),
	service("svc-brkr", "Credential broker · brkr", "key", "ask"),
	service("svc-kyz", "Secrets vault · kyz", "database", "ask"),
	service("svc-ssh", "SSH agent access", "terminal", "ask"),
	service("svc-eavs", "Model access · EAVS", "globe", "allow"),
	service("svc-egress", "Egress firewall", "shield", "allow"),
	service("svc-registry", "Container registry", "code", "ask"),
];

/** Configured connections: discovery hints, never grants. */
export const connections: Readonly<Record<string, readonly string[]>> = {
	oqto: [
		"wiki-eng",
		"wiki-private",
		"mem-personal",
		"svc-brkr",
		"svc-kyz",
		"svc-ssh",
		"svc-eavs",
		"svc-registry",
	],
	"oqt-cli": [
		"wiki-eng",
		"mem-personal",
		"svc-brkr",
		"svc-kyz",
		"svc-ssh",
		"svc-eavs",
	],
	"pi-extensions": ["wiki-eng", "svc-brkr", "svc-eavs", "svc-registry"],
	dotfiles: ["wiki-private", "svc-brkr", "svc-eavs"],
	notes: [
		"wiki-private",
		"wiki-research",
		"mem-personal",
		"svc-kyz",
		"svc-eavs",
	],
	"web-app": [
		"wiki-eng",
		"wiki-product",
		"mem-product",
		"svc-brkr",
		"svc-kyz",
		"svc-eavs",
		"svc-registry",
		"svc-egress",
	],
	"ios-app": [
		"wiki-product",
		"mem-product",
		"svc-brkr",
		"svc-eavs",
		"svc-registry",
	],
	"android-app": [
		"wiki-product",
		"mem-product",
		"svc-brkr",
		"svc-eavs",
		"svc-registry",
	],
	"design-system": ["wiki-product", "svc-eavs"],
	"api-gateway": [
		"wiki-eng",
		"wiki-product",
		"svc-brkr",
		"svc-kyz",
		"svc-eavs",
		"svc-registry",
		"svc-egress",
	],
	"rag-eval": [
		"wiki-research",
		"mem-research",
		"svc-brkr",
		"svc-eavs",
		"svc-egress",
	],
	benchmarks: ["wiki-research", "wiki-eng", "svc-eavs", "svc-registry"],
	paper: ["wiki-research", "svc-eavs"],
	portal: ["wiki-client", "mem-client", "svc-brkr", "svc-eavs", "svc-egress"],
	pipeline: ["wiki-client", "mem-client", "svc-kyz", "svc-eavs", "svc-egress"],
	audit: ["wiki-client", "svc-eavs"],
};
export const byId = (id: string) => resources.find((item) => item.id === id);
export const workspaceOf = (id: string) => byId(id)?.parent;
