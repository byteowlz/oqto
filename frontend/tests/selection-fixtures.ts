import type { SelectionRecord } from "../src/oqto-ui/platform/selection-contract";
import { createSelectionController } from "../src/oqto-ui/platform/selection-controller";
import type {
	SelectionOperation,
	SelectionPort,
} from "../src/oqto-ui/platform/selection-port";
import {
	emptyAnswers,
	normalizeSpec,
} from "../src/oqto-ui/platform/selection-validation";

export function review(
	mode: "questions" | "review" = "questions",
): SelectionRecord {
	const spec = normalizeSpec(
		mode === "questions"
			? {
					version: 1,
					mode,
					title: "Decisions",
					questions: [
						{
							id: "direction",
							title: "Direction",
							kind: "single",
							options: [
								{ id: "small", label: "Small", recommended: true },
								{ id: "full", label: "Full" },
							],
							allowOther: true,
							note: { label: "Reason", multiline: true },
						},
						{
							id: "features",
							title: "Features",
							kind: "multiple",
							options: [
								{ id: "a", label: "Alpha" },
								{ id: "b", label: "Beta" },
							],
						},
						{
							id: "summary",
							title: "Summary",
							kind: "text",
							multiline: true,
							required: true,
						},
					],
				}
			: {
					version: 1,
					mode,
					title: "Inventory",
					groups: [
						{ id: "one", title: "First" },
						{ id: "two", title: "Second" },
					],
					items: [
						{ id: "apple", label: "Apple", groupId: "one" },
						{ id: "pear", label: "Pear", groupId: "two" },
					],
					choices: [
						{ id: "keep", label: "Keep" },
						{ id: "drop", label: "Drop" },
					],
					allowOther: true,
					note: { label: "Reason" },
				},
	);
	return {
		version: 1,
		id: "review-one",
		scopeId: "native-scope",
		revision: 0,
		state: "draft",
		spec,
		answers: emptyAnswers(spec),
		createdAt: "2026-01-01",
		updatedAt: "2026-01-01",
	};
}
export function selectionHarness(initial = review()) {
	let record = initial;
	let present: (value: unknown) => void = () => {};
	let connection: (online: boolean) => void = () => {};
	let renew: () => void = () => {};
	let savedPointer: { scopeId: string; id: string } | null = null;
	const calls: Array<{
		operation: SelectionOperation;
		argumentsValue: object;
	}> = [];
	let discovered = true;
	let failure: string | null = null;
	let delayed: ((value: unknown) => void) | null = null;
	let delayNext = false;
	const port: SelectionPort = {
		discover: async () => discovered,
		async control(operation, argumentsValue) {
			calls.push({ operation, argumentsValue });
			if (failure) throw new Error(failure);
			if (delayNext) {
				delayNext = false;
				return new Promise((resolve) => {
					delayed = resolve;
				});
			}
			if (operation === "presentation_bind")
				return {
					bindingId: "connection-binding",
					snapshot: {
						scopeId: record.scopeId,
						bindings: [
							{
								id: "another-binding",
								clientKind: "oqto-web",
								expiresAt: 100000,
							},
							{
								id: "connection-binding",
								clientKind: "oqto-web",
								expiresAt: 100000,
							},
						],
					},
				};
			if (operation === "selection_get") return { record };
			if (operation === "selection_close") {
				record = {
					...record,
					state: "cancelled",
					revision: record.revision + 1,
				};
				return { record };
			}
			if (operation === "selection_save" || operation === "selection_submit") {
				const args = argumentsValue as {
					revision: number;
					answers: SelectionRecord["answers"];
				};
				if (args.revision !== record.revision)
					throw new Error("Revision conflict");
				record = {
					...record,
					revision: record.revision + 1,
					answers: args.answers,
					state: operation === "selection_submit" ? "submitted" : "draft",
				};
				return record;
			}
			return { ok: true };
		},
		listen(onPresent, onConnection) {
			present = onPresent;
			connection = onConnection;
			connection(true);
			return () => {
				present = () => {};
				connection = () => {};
			};
		},
		renew(callback) {
			renew = callback;
			return () => {
				renew = () => {};
			};
		},
		resume(scopeId) {
			return savedPointer?.scopeId === scopeId ? savedPointer.id : null;
		},
		remember(scopeId, id) {
			savedPointer = { scopeId, id };
		},
	};
	const controller = createSelectionController(port);
	return {
		controller,
		port,
		calls,
		present(
			value: unknown = {
				version: 1,
				bindingIds: ["connection-binding"],
				review: record,
			},
		) {
			present(value);
		},
		connection(online: boolean) {
			connection(online);
		},
		renew() {
			renew();
		},
		unavailable() {
			discovered = false;
		},
		fail(message: string | null) {
			failure = message;
		},
		delay() {
			delayNext = true;
		},
		resolve(value: unknown) {
			delayed?.(value);
		},
		serverRecord(value: SelectionRecord) {
			record = value;
		},
		record: () => record,
	};
}
export async function settle() {
	for (let i = 0; i < 10; i++) await Promise.resolve();
}
