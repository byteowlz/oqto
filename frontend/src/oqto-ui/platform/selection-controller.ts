import type { JsonValue } from "../engine/projection";
import type { SelectionRecord } from "./selection-contract";
import type { SelectionController, SelectionSnapshot } from "./selection-host";
import type { SelectionPort } from "./selection-port";
import {
	validateAnswers,
	validateNormalized,
	validateSelectionAnswer,
} from "./selection-validation";

function object(value: unknown): Record<string, JsonValue | undefined> {
	if (!value || typeof value !== "object" || Array.isArray(value))
		throw new Error("Invalid selection response");
	return value as Record<string, JsonValue | undefined>;
}
export function parseSelectionRecord(value: unknown): SelectionRecord {
	const row = object(value);
	const keys = [
		"version",
		"id",
		"scopeId",
		"revision",
		"state",
		"spec",
		"answers",
		"createdAt",
		"updatedAt",
	];
	if (
		Object.keys(row).some((key) => !keys.includes(key)) ||
		row.version !== 1 ||
		typeof row.id !== "string" ||
		!row.id ||
		row.id.length > 128 ||
		typeof row.scopeId !== "string" ||
		!row.scopeId ||
		row.scopeId.length > 512 ||
		!Number.isSafeInteger(row.revision) ||
		Number(row.revision) < 0 ||
		!["draft", "submitted", "cancelled"].includes(String(row.state)) ||
		typeof row.createdAt !== "string" ||
		typeof row.updatedAt !== "string"
	)
		throw new Error("Unsupported selection record");
	const spec = validateNormalized(row.spec);
	return {
		version: 1,
		id: row.id,
		scopeId: row.scopeId,
		revision: Number(row.revision),
		state: row.state as SelectionRecord["state"],
		spec,
		answers: validateAnswers(spec, row.answers, row.state === "submitted"),
		createdAt: row.createdAt,
		updatedAt: row.updatedAt,
	};
}

/** Domain state is shared by all authorized presentations of this Session, not by layout. */
export function createSelectionController(
	port: SelectionPort,
): SelectionController {
	let state: SelectionSnapshot = {
		phase: "connecting",
		record: null,
		answers: {},
		dirty: false,
		busy: false,
		error: null,
		fieldErrors: {},
		presentationEpoch: 0,
	};
	const listeners = new Set<() => void>();
	let generation = 0;
	let mutation = 0;
	let dispose: (() => void) | null = null;
	let cancelRenew: (() => void) | null = null;
	let bindingId: string | null = null;
	let renderedKey: string | null = null;
	let nativeScope: string | null = null;
	let connected = false;
	const publish = (patch: Partial<SelectionSnapshot>) => {
		state = { ...state, ...patch };
		for (const listener of listeners) listener();
	};
	const accept = (value: unknown) => {
		const record = parseSelectionRecord(value);
		if (nativeScope && nativeScope !== record.scopeId)
			throw new Error("Selection scope mismatch");
		nativeScope = record.scopeId;
		port.remember(record.scopeId, record.id);
		if (
			state.record?.id === record.id &&
			state.record.revision > record.revision
		)
			return;
		publish({
			record,
			answers: record.answers,
			dirty: false,
			error: null,
			fieldErrors: {},
		});
	};
	const fail = (error: unknown) =>
		publish({
			error:
				error instanceof Error ? error.message : "Selection control failed",
		});
	const bind = async (epoch: number) => {
		bindingId = null;
		renderedKey = null;
		publish({ phase: "connecting" });
		try {
			if (!(await port.discover())) {
				if (epoch === generation) publish({ phase: "unavailable" });
				return;
			}
			if (epoch !== generation) return;
			const reply = object(
				await port.control("presentation_bind", {
					clientKind: "oqto-web",
					capabilities: ["selection.questions.v1", "selection.review.v1"],
					leaseMs: 60000,
				}),
			);
			if (epoch !== generation) {
				if (!listeners.size)
					void port.control("presentation_unbind", {}).catch(() => {});
				return;
			}
			const snapshot = object(reply.snapshot);
			if (
				typeof snapshot.scopeId !== "string" ||
				!Array.isArray(snapshot.bindings)
			)
				throw new Error("Invalid presentation lease");
			// Backend supplies the authenticated binding, never an agent's metadata claim.
			const binding = snapshot.bindings
				.map(object)
				.find(
					(row) => row.id === reply.bindingId && row.clientKind === "oqto-web",
				);
			if (
				!binding ||
				typeof binding.id !== "string" ||
				typeof binding.expiresAt !== "number"
			)
				throw new Error("Missing presentation lease");
			bindingId = binding.id;
			nativeScope = snapshot.scopeId;
			if (state.record && state.record.scopeId !== nativeScope)
				publish({ record: null, answers: {}, dirty: false });
			publish({
				phase: "ready",
				error: null,
				presentationEpoch: state.presentationEpoch + 1,
			});
			cancelRenew?.();
			cancelRenew = port.renew(() => {
				if (epoch === generation) void bind(epoch);
			});
			const resumeId = state.record?.id ?? port.resume(nativeScope);
			if (resumeId && !state.dirty) await controller.reload(resumeId);
		} catch (error) {
			if (epoch === generation) {
				bindingId = null;
				publish({ phase: "unavailable" });
				fail(error);
			}
		}
	};
	const controller: SelectionController = {
		snapshot: () => state,
		rendered(reviewId) {
			if (
				!bindingId ||
				state.phase !== "ready" ||
				state.record?.id !== reviewId ||
				state.record.state !== "draft"
			)
				return;
			const epoch = generation;
			const key = `${epoch}:${state.presentationEpoch}:${reviewId}`;
			if (renderedKey === key) return;
			renderedKey = key;
			void port.control("selection_ack", { id: reviewId }).catch((error) => {
				if (epoch === generation && state.record?.id === reviewId) {
					renderedKey = null;
					fail(error);
				}
			});
		},
		subscribe(listener) {
			listeners.add(listener);
			if (listeners.size === 1) {
				dispose = port.listen(
					(value) => {
						try {
							const payload = object(value);
							if (
								payload.version !== 1 ||
								!Array.isArray(payload.bindingIds) ||
								!bindingId ||
								!payload.bindingIds.includes(bindingId) ||
								state.phase !== "ready" ||
								state.busy
							)
								return;
							const record = parseSelectionRecord(payload.review);
							if (nativeScope !== record.scopeId)
								throw new Error("Selection scope mismatch");
							if (state.dirty && state.record?.id === record.id) return;
							accept(record);
						} catch (error) {
							fail(error);
						}
					},
					(online) => {
						connected = online;
						generation++;
						bindingId = null;
						cancelRenew?.();
						cancelRenew = null;
						publish({ phase: "connecting", busy: false });
						if (online) void bind(generation);
					},
				);
			}
			return () => {
				listeners.delete(listener);
				if (listeners.size === 0) {
					generation++;
					mutation++;
					dispose?.();
					dispose = null;
					cancelRenew?.();
					cancelRenew = null;
					if (connected && bindingId)
						void port.control("presentation_unbind", {}).catch(() => {});
					bindingId = null;
					nativeScope = null;
					connected = false;
					state = { ...state, phase: "connecting", busy: false, error: null };
				}
			};
		},
		edit(answers) {
			if (state.record?.state !== "draft" || state.busy) return;
			publish({ answers, dirty: true, error: null, fieldErrors: {} });
		},
		async save(submit = false) {
			const record = state.record;
			if (
				!record ||
				record.state !== "draft" ||
				state.phase !== "ready" ||
				state.busy
			)
				return;
			const epoch = generation;
			const ticket = ++mutation;
			try {
				const fieldErrors: Record<string, string> = {};
				for (const question of record.spec.questions) {
					try {
						validateSelectionAnswer(
							question,
							state.answers[question.id] ?? {
								answered: false,
								selectedIds: [],
							},
							submit,
						);
					} catch (error) {
						fieldErrors[question.id] =
							error instanceof Error ? error.message : "Invalid answer";
					}
				}
				if (Object.keys(fieldErrors).length) {
					publish({ fieldErrors });
					throw new Error(Object.values(fieldErrors)[0]);
				}
				const answers = validateAnswers(record.spec, state.answers, submit);
				publish({ busy: true, error: null, fieldErrors: {} });
				const result = await port.control(
					submit ? "selection_submit" : "selection_save",
					{ id: record.id, revision: record.revision, answers },
				);
				if (epoch === generation && ticket === mutation) {
					const saved = parseSelectionRecord(result);
					if (
						saved.id !== record.id ||
						saved.scopeId !== record.scopeId ||
						saved.revision <= record.revision
					)
						throw new Error("Stale selection acknowledgement");
					accept(saved);
				}
			} catch (error) {
				if (epoch === generation && ticket === mutation) fail(error);
			} finally {
				if (epoch === generation && ticket === mutation)
					publish({ busy: false });
			}
		},
		async reload(id) {
			const reviewId = id ?? state.record?.id;
			if (!reviewId) {
				if (connected && !state.busy) await bind(generation);
				return;
			}
			if (state.phase !== "ready" || state.busy) return;
			const epoch = generation;
			const ticket = ++mutation;
			publish({ busy: true });
			try {
				const result = object(
					await port.control("selection_get", { id: reviewId }),
				);
				if (epoch === generation && ticket === mutation) {
					const loaded = parseSelectionRecord(result.record);
					if (loaded.id !== reviewId)
						throw new Error("Mismatched review acknowledgement");
					accept(loaded);
				}
			} catch (error) {
				if (epoch === generation && ticket === mutation) fail(error);
			} finally {
				if (epoch === generation && ticket === mutation)
					publish({ busy: false });
			}
		},
		async close() {
			const record = state.record;
			if (
				!record ||
				state.phase !== "ready" ||
				state.busy ||
				record.state !== "draft"
			)
				return;
			const epoch = generation;
			const ticket = ++mutation;
			publish({ busy: true });
			try {
				const result = object(
					await port.control("selection_close", {
						id: record.id,
						revision: record.revision,
					}),
				);
				if (epoch === generation && ticket === mutation) {
					const closed = parseSelectionRecord(result.record);
					if (closed.id !== record.id || closed.state !== "cancelled")
						throw new Error("Mismatched close acknowledgement");
					accept(closed);
				}
			} catch (error) {
				if (epoch === generation && ticket === mutation) fail(error);
			} finally {
				if (epoch === generation && ticket === mutation)
					publish({ busy: false });
			}
		},
	};
	return controller;
}
