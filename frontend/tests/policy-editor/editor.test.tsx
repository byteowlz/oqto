import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import i18n from "i18next";
import { initReactI18next } from "react-i18next";
import { afterEach, describe, expect, it } from "vitest";
void i18n
	.use(initReactI18next)
	.init({ lng: "en", resources: {}, interpolation: { escapeValue: false } });
import { fixture as containmentFixture } from "../../src/oqto-ui/dev/policy-editor/fixtures";
import { PolicyTree } from "../../src/oqto-ui/policy-editor/PolicyTree";
// Simulation only: no real composed authority or live adapter is implemented.
const fixture = {
	...containmentFixture,
	projection: "containment_and_guard" as const,
};
import {
	type Ack,
	type Approval,
	type Binding,
	type Preview,
	createPolicyEditor,
} from "../../src/oqto-ui/policy-editor/model";
afterEach(cleanup);
const deferred = <T,>() => {
	let resolve: (v: T) => void = () => {};
	const promise = new Promise<T>((r) => {
		resolve = r;
	});
	return { promise, resolve };
};
describe("permission boundary presentation", () => {
	it("preserves simultaneous inherited/mixed and locked-descendant observations", () => {
		const current = {
			...containmentFixture,
			nodes: [
				{
					...containmentFixture.nodes[0],
					inherited: true,
					mixed: true,
					containsLockedDescendant: true,
				},
			],
		};
		const editor = createPolicyEditor(current);
		render(<PolicyTree editor={editor} />);
		expect(screen.getByRole("treeitem")).toHaveTextContent("inherited");
		expect(screen.getByRole("treeitem")).toHaveTextContent("mixed");
		expect(screen.getByRole("treeitem")).toHaveTextContent(
			"Contains locked descendants",
		);
		expect(screen.getByText(/Containment projection only/)).toBeInTheDocument();
	});
	it("partial containment never enables apply even with simulated receipt", async () => {
		const editor = createPolicyEditor(containmentFixture, {
			preview: async (b) => ({
				...b,
				nodes: containmentFixture.nodes,
				diff: [],
			}),
			apply: async () => {
				throw new Error("must not invoke writer");
			},
		});
		editor.edit({ nodeId: "src", mode: "read", source: "operator" });
		await editor.preview();
		const p = editor.getSnapshot().preview;
		if (!p) throw new Error("missing preview");
		editor.acceptApproval({ ...p, receipt: "not sufficient" });
		expect(editor.canApply()).toBe(false);
		await editor.apply();
		expect(editor.getSnapshot().applied).toBeUndefined();
	});
	it("mouse and keyboard expose the same nodes; locked controls cannot edit", () => {
		const editor = createPolicyEditor(fixture);
		render(<PolicyTree editor={editor} />);
		fireEvent.click(screen.getByRole("button", { name: "Edit boundaries" }));
		const root = screen.getAllByRole("treeitem")[0];
		expect(screen.getAllByRole("treeitem")).toHaveLength(3);
		fireEvent.keyDown(root, { key: "ArrowLeft" });
		expect(screen.getAllByRole("treeitem")).toHaveLength(1);
		fireEvent.keyDown(root, { key: "ArrowRight" });
		expect(screen.getAllByRole("treeitem")).toHaveLength(3);
		fireEvent.keyDown(root, { key: "ArrowDown" });
		expect(screen.getAllByRole("treeitem")[1]).toHaveFocus();
		fireEvent.click(screen.getByRole("button", { name: "read" }));
		expect(editor.getSnapshot().edits[0]?.mode).toBe("read");
		fireEvent.click(screen.getAllByRole("treeitem")[2]);
		expect(screen.getByRole("button", { name: "read" })).toBeDisabled();
		expect(
			editor.edit({ nodeId: "secrets", mode: "read", source: "interviewer" }),
		).toBe(false);
		fireEvent.click(
			screen.getByRole("button", { name: "Collapse example-repo" }),
		);
		expect(screen.getAllByRole("treeitem")).toHaveLength(1);
		fireEvent.click(
			screen.getByRole("button", { name: "Expand example-repo" }),
		);
		expect(screen.getAllByRole("treeitem")).toHaveLength(3);
		expect(
			screen.getByRole("button", { name: "Apply reviewed draft" }),
		).toBeDisabled();
	});
	it("never evaluates a proposed mode into current effective policy", () => {
		const editor = createPolicyEditor(fixture);
		editor.edit({ nodeId: "src", mode: "deny", source: "interviewer" });
		expect(editor.getSnapshot().current).toBe(fixture);
		expect(editor.getSnapshot().applied).toBeUndefined();
		expect(editor.canApply()).toBe(false);
		expect(
			editor.edit({ nodeId: "src", mode: "ask", source: "operator" }),
		).toBe(false);
	});
	it("ignores stale preview and invalidates approval on proposal mutation", async () => {
		const wait = deferred<Preview>();
		let binding: Binding | undefined;
		const editor = createPolicyEditor(fixture, {
			preview: (b) => {
				binding = b;
				return wait.promise;
			},
			apply: async () => {
				throw new Error("not called");
			},
		});
		editor.edit({ nodeId: "src", mode: "read", source: "operator" });
		const pending = editor.preview();
		editor.edit({ nodeId: "src", mode: "deny", source: "interviewer" });
		if (!binding) throw new Error("missing binding");
		wait.resolve({ ...binding, nodes: fixture.nodes, diff: [] });
		await pending;
		expect(editor.getSnapshot().preview).toBeUndefined();
		expect(editor.acceptApproval({ ...binding, receipt: "protected" })).toBe(
			false,
		);
	});
	it("requires exact ack; ignores stale apply result and separates proposed/applied", async () => {
		const wait = deferred<Ack>();
		let approved: Approval | undefined;
		const editor = createPolicyEditor(fixture, {
			preview: async (b) => ({
				...b,
				nodes: fixture.nodes,
				diff: ["synthetic preview"],
			}),
			apply: (b, approval) => {
				approved = { ...b, receipt: approval.receipt };
				return wait.promise;
			},
		});
		editor.edit({ nodeId: "src", mode: "read", source: "operator" });
		await editor.preview();
		const preview = editor.getSnapshot().preview;
		if (!preview) throw new Error("missing preview");
		expect(editor.canApply()).toBe(false);
		editor.acceptApproval({ ...preview, receipt: "protected" });
		expect(editor.canApply()).toBe(true);
		const applying = editor.apply();
		expect(editor.getSnapshot().applied).toBeUndefined();
		editor.edit({ nodeId: "src", mode: "deny", source: "interviewer" });
		if (!approved) throw new Error("missing approval");
		wait.resolve({
			...approved,
			status: "acked",
			adoptionGeneration: "g2",
			snapshot: { ...fixture, revision: "r2" },
		});
		await applying;
		expect(editor.getSnapshot().applied).toBeUndefined();
		expect(editor.getSnapshot().approval).toBeUndefined();
	});
	it("invalidates an existing protected approval on either source of edits", async () => {
		const editor = createPolicyEditor(fixture, {
			preview: async (b) => ({ ...b, nodes: fixture.nodes, diff: [] }),
			apply: async () => {
				throw new Error("must not apply");
			},
		});
		for (const source of ["operator", "interviewer"] as const) {
			editor.edit({ nodeId: "src", mode: "read", source });
			await editor.preview();
			const p = editor.getSnapshot().preview;
			if (!p) throw new Error("missing preview");
			expect(editor.acceptApproval({ ...p, receipt: "protected" })).toBe(true);
			editor.edit({ nodeId: "src", mode: "deny", source });
			expect(editor.canApply()).toBe(false);
			expect(editor.getSnapshot().approval).toBeUndefined();
		}
	});
	it("uncertain publication freezes the draft without claiming no effects or applied", async () => {
		const editor = createPolicyEditor(fixture, {
			preview: async (b) => ({ ...b, nodes: fixture.nodes, diff: [] }),
			apply: async (b) => ({
				...b,
				status: "publication_uncertain",
				receipt: "",
				snapshot: fixture,
			}),
		});
		editor.edit({ nodeId: "src", mode: "read", source: "operator" });
		await editor.preview();
		const p = editor.getSnapshot().preview;
		if (!p) throw new Error("missing preview");
		editor.acceptApproval({ ...p, receipt: "simulated" });
		await editor.apply();
		expect(editor.getSnapshot().publicationUncertain?.status).toBe(
			"publication_uncertain",
		);
		expect(editor.getSnapshot().applied).toBeUndefined();
		expect(
			editor.edit({ nodeId: "src", mode: "deny", source: "operator" }),
		).toBe(false);
	});
	it("published_not_adopted never claims applied or allows another edit", async () => {
		const editor = createPolicyEditor(fixture, {
			preview: async (b) => ({ ...b, nodes: fixture.nodes, diff: [] }),
			apply: async (b) => ({
				...b,
				status: "published_not_adopted",
				receipt: "publication",
				snapshot: { ...fixture, revision: "disk-r2" },
			}),
		});
		editor.edit({ nodeId: "src", mode: "read", source: "operator" });
		await editor.preview();
		const p = editor.getSnapshot().preview;
		if (!p) throw new Error("missing preview");
		editor.acceptApproval({ ...p, receipt: "protected" });
		await editor.apply();
		expect(editor.getSnapshot().publishedNotAdopted?.status).toBe(
			"published_not_adopted",
		);
		expect(editor.getSnapshot().applied).toBeUndefined();
		expect(editor.getSnapshot().current.revision).toBe("fixture-r1");
		expect(
			editor.edit({ nodeId: "src", mode: "deny", source: "operator" }),
		).toBe(false);
	});
	it("only marks applied after authoritative acknowledgement", async () => {
		const editor = createPolicyEditor(fixture, {
			preview: async (b) => ({ ...b, nodes: fixture.nodes, diff: [] }),
			apply: async (b) => ({
				...b,
				receipt: "ack",
				status: "acked",
				adoptionGeneration: "g2",
				snapshot: { ...fixture, revision: "r2" },
			}),
		});
		editor.edit({ nodeId: "src", mode: "read", source: "operator" });
		await editor.preview();
		const p = editor.getSnapshot().preview;
		if (!p) throw new Error("missing preview");
		editor.acceptApproval({ ...p, receipt: "protected" });
		await editor.apply();
		expect(editor.getSnapshot().applied?.revision).toBe("r2");
		expect(editor.getSnapshot().edits).toEqual([]);
	});
});
