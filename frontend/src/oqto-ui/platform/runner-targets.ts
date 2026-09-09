export type RunnerTarget = {
	id: string;
	label: string;
	connection: "online" | "unavailable" | "incompatible";
	checkedAt: string;
	/** Connectivity is not permission: this is an explicit execution grant. */
	sessionCreation: boolean;
	providerLogin?: boolean;
	historyRead?: boolean;
};

export type RunnerTargetsPort = {
	/** Host identity for query isolation; no transport addresses or credentials. */
	id: string;
	list: () => Promise<RunnerTarget[]>;
};

type RunnerTargetWire = {
	id?: unknown;
	label?: unknown;
	connection?: unknown;
	checked_at?: unknown;
	session_creation?: unknown;
	provider_login?: unknown;
	history_read?: unknown;
};

export function parseRunnerTargets(value: unknown): RunnerTarget[] {
	if (!Array.isArray(value)) throw new Error("Invalid runner target list");
	const ids = new Set<string>();
	return value.map((item: unknown) => {
		if (!item || typeof item !== "object")
			throw new Error("Invalid runner target");
		const row = item as RunnerTargetWire;
		if (
			typeof row.id !== "string" ||
			!row.id ||
			ids.has(row.id) ||
			typeof row.label !== "string" ||
			!row.label ||
			(row.connection !== "online" &&
				row.connection !== "unavailable" &&
				row.connection !== "incompatible") ||
			typeof row.checked_at !== "string" ||
			!Number.isFinite(Date.parse(row.checked_at)) ||
			typeof row.session_creation !== "boolean"
		)
			throw new Error("Invalid runner target capabilities");
		ids.add(row.id);
		return {
			id: row.id,
			label: row.label,
			connection: row.connection,
			checkedAt: row.checked_at,
			sessionCreation: row.session_creation,
			...(row.provider_login === true ? { providerLogin: true } : {}),
			...(row.history_read === true ? { historyRead: true } : {}),
		};
	});
}
