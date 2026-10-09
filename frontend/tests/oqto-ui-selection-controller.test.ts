import { describe, expect, it } from "vitest";
import { parseSelectionRecord } from "../src/oqto-ui/platform/selection-controller";
import { review, selectionHarness, settle } from "./selection-fixtures";

describe("selection state authority", () => {
	it("discovers before binding; absent commands never fall through", async () => {
		const h = selectionHarness();
		h.unavailable();
		const stop = h.controller.subscribe(() => {});
		await settle();
		expect(h.controller.snapshot().phase).toBe("unavailable");
		expect(h.calls).toEqual([]);
		stop();
	});
	it("accepts only the actual owning binding/native scope, not a first web client's metadata", async () => {
		const h = selectionHarness();
		const stop = h.controller.subscribe(() => {});
		await settle();
		h.present({
			version: 1,
			bindingIds: ["another-binding"],
			review: h.record(),
		});
		expect(h.controller.snapshot().record).toBeNull();
		h.present({
			version: 1,
			bindingIds: ["connection-binding"],
			review: { ...h.record(), scopeId: "platform-id" },
		});
		expect(h.controller.snapshot().error).toMatch(/scope/);
		h.present();
		expect(h.controller.snapshot().record?.scopeId).toBe("native-scope");
		expect(h.calls.some((call) => call.operation === "selection_ack")).toBe(
			false,
		);
		h.controller.rendered("review-one");
		expect(h.calls.at(-1)?.operation).toBe("selection_ack");
		stop();
	});
	it("shares draft state across views and unbinds only after the last view", async () => {
		const h = selectionHarness();
		const a = h.controller.subscribe(() => {});
		const b = h.controller.subscribe(() => {});
		await settle();
		h.present();
		expect(
			h.calls.filter((call) => call.operation === "presentation_bind"),
		).toHaveLength(1);
		a();
		expect(
			h.calls.some((call) => call.operation === "presentation_unbind"),
		).toBe(false);
		b();
		expect(h.calls.at(-1)?.operation).toBe("presentation_unbind");
		expect(h.controller.snapshot().phase).toBe("connecting");
		expect(h.controller.snapshot().record?.id).toBe("review-one");
	});
	it("keeps save distinct from submit, validates required fields, and treats terminal records as immutable", async () => {
		const h = selectionHarness();
		const stop = h.controller.subscribe(() => {});
		await settle();
		h.present();
		h.controller.edit({
			...h.controller.snapshot().answers,
			features: { answered: true, selectedIds: [] },
			direction: { answered: false, disposition: "unsure", selectedIds: [] },
		});
		await h.controller.save();
		expect(h.record().state).toBe("draft");
		expect(h.record().answers.features.answered).toBe(true);
		await h.controller.save(true);
		expect(h.controller.snapshot().error).toMatch(/Required/);
		expect(h.record().state).toBe("draft");
		h.controller.edit({
			...h.controller.snapshot().answers,
			summary: { answered: true, selectedIds: [], text: "Done" },
		});
		await h.controller.save(true);
		expect(h.record().state).toBe("submitted");
		const count = h.calls.length;
		await h.controller.close();
		await h.controller.save();
		expect(h.calls).toHaveLength(count);
		stop();
	});
	it("preserves unsaved answers on CAS failure and requires explicit reload", async () => {
		const h = selectionHarness();
		const stop = h.controller.subscribe(() => {});
		await settle();
		h.present();
		h.controller.edit({
			...h.controller.snapshot().answers,
			summary: { answered: true, selectedIds: [], text: "local" },
		});
		h.serverRecord({ ...h.record(), revision: 2 });
		await h.controller.save();
		expect(h.controller.snapshot().error).toBe("Revision conflict");
		expect(h.controller.snapshot().answers.summary.text).toBe("local");
		await h.controller.reload();
		expect(h.controller.snapshot().record?.revision).toBe(2);
		expect(h.controller.snapshot().dirty).toBe(false);
		stop();
	});
	it("ignores in-flight stale replies after disconnect/session disposal", async () => {
		const h = selectionHarness();
		const stop = h.controller.subscribe(() => {});
		await settle();
		h.present();
		h.delay();
		const request = h.controller.save();
		await settle();
		h.connection(false);
		h.resolve({ ...h.record(), revision: 1, state: "submitted" });
		await request;
		expect(h.controller.snapshot().record?.state).toBe("draft");
		expect(h.controller.snapshot().phase).toBe("connecting");
		stop();
		h.present();
		expect(h.controller.snapshot().record?.revision).toBe(0);
	});
	it("renews leases; failed renewal gates mutation; reconnect reloads the durable record", async () => {
		const h = selectionHarness();
		const stop = h.controller.subscribe(() => {});
		await settle();
		h.present();
		h.fail("lease expired");
		h.renew();
		await settle();
		expect(h.controller.snapshot().phase).toBe("unavailable");
		const calls = h.calls.length;
		await h.controller.save();
		expect(h.calls).toHaveLength(calls);
		h.fail(null);
		h.serverRecord({ ...h.record(), revision: 3 });
		h.connection(false);
		h.connection(true);
		await settle();
		expect(h.controller.snapshot().record?.revision).toBe(3);
		stop();
	});
	it("restores only scoped review pointers and isolates sessions", async () => {
		const a = selectionHarness();
		const b = selectionHarness({
			...review(),
			id: "other",
			scopeId: "other-scope",
		});
		const stopA = a.controller.subscribe(() => {});
		const stopB = b.controller.subscribe(() => {});
		await settle();
		a.present();
		expect(b.controller.snapshot().record).toBeNull();
		stopA();
		const resume = a.controller.subscribe(() => {});
		await settle();
		expect(a.controller.snapshot().record?.id).toBe("review-one");
		resume();
		stopB();
	});
	it("preserves local drafts across detached placement and reconnect, but never across native scopes", async () => {
		const h = selectionHarness();
		const stop = h.controller.subscribe(() => {});
		await settle();
		h.present();
		h.controller.edit({
			...h.controller.snapshot().answers,
			summary: { answered: true, selectedIds: [], text: "Unsaved" },
		});
		stop();
		const resume = h.controller.subscribe(() => {});
		await settle();
		expect(h.controller.snapshot().answers.summary.text).toBe("Unsaved");
		h.connection(false);
		h.serverRecord({
			...review(),
			scopeId: "different-native-scope",
			id: "new-review",
		});
		h.connection(true);
		await settle();
		expect(h.controller.snapshot().record).toBeNull();
		expect(h.controller.snapshot().dirty).toBe(false);
		resume();
	});
	it("rejects a stale/wrong-review successful mutation acknowledgement", async () => {
		const h = selectionHarness();
		const stop = h.controller.subscribe(() => {});
		await settle();
		h.present();
		h.delay();
		const request = h.controller.save();
		await settle();
		h.resolve({ ...h.record(), id: "another", revision: 1 });
		await request;
		expect(h.controller.snapshot().record?.id).toBe("review-one");
		expect(h.controller.snapshot().error).toMatch(/Stale/);
		stop();
	});
	it("bounds field constraints on drafts and names invalid required fields", async () => {
		const initial = review();
		initial.spec.questions[0].maxLength = 3;
		initial.spec.questions[1].maxSelections = 1;
		const h = selectionHarness(initial);
		const stop = h.controller.subscribe(() => {});
		await settle();
		h.present();
		h.controller.edit({
			...h.controller.snapshot().answers,
			direction: { answered: true, selectedIds: [], text: "too long" },
			features: { answered: true, selectedIds: ["a", "b"] },
		});
		await h.controller.save();
		expect(Object.keys(h.controller.snapshot().fieldErrors)).toEqual([
			"direction",
			"features",
		]);
		expect(h.calls.some((row) => row.operation === "selection_save")).toBe(
			false,
		);
		stop();
	});
	it("fails closed on unknown schema, mode, fields, choices and dispositions", () => {
		const valid = review();
		expect(() => parseSelectionRecord({ ...valid, version: 2 })).toThrow();
		expect(() =>
			parseSelectionRecord({ ...valid, spec: { ...valid.spec, mode: "html" } }),
		).toThrow();
		expect(() =>
			parseSelectionRecord({ ...valid, endpoint: "https://invalid" }),
		).toThrow();
		expect(() =>
			parseSelectionRecord({
				...valid,
				answers: { direction: { answered: true, selectedIds: ["unknown"] } },
			}),
		).toThrow();
		expect(() =>
			parseSelectionRecord({
				...valid,
				answers: {
					direction: {
						answered: true,
						disposition: "skipped",
						selectedIds: [],
					},
				},
			}),
		).toThrow();
	});
});
