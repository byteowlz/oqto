import type { OqtoUiPlatform } from "@/src/oqto-ui/platform/contracts";
import { createFlashHosts } from "@/src/oqto-ui/platform/flash-hosts";
import { describe, expect, it, vi } from "vitest";

function fixture() {
	const worker = {
		list: vi.fn(async (path: string) =>
			path === "/workspace"
				? [
						{
							path: "/workspace/src",
							name: "src",
							kind: "directory",
							size: 0,
							mtimeMs: 100,
						},
						{
							path: "/workspace/readme.md",
							name: "readme.md",
							kind: "file",
							size: 42,
							mtimeMs: 1234,
						},
						{
							path: "/workspace/.hidden",
							name: ".hidden",
							kind: "file",
							size: 1,
							mtimeMs: 100,
						},
					]
				: [
						{
							path: "/workspace/src/a.ts",
							name: "a.ts",
							kind: "file",
							size: 7,
							mtimeMs: 200,
						},
					],
		),
		subscribe: vi.fn(() => () => {}),
		read: vi.fn(async () => "actual worker contents"),
	};
	const hosts: Pick<
		OqtoUiPlatform,
		"files" | "issues" | "git" | "terminal" | "actions"
	> = createFlashHosts(worker);
	return { worker, hosts };
}

describe("Flash hosts", () => {
	it("projects actual Worker paths into lazy single-level listings", async () => {
		const { hosts, worker } = fixture();
		const root = await hosts.files.list("/workspace", "");
		expect(root.map(({ path, directory }) => ({ path, directory }))).toEqual([
			{ path: "src", directory: true },
			{ path: "readme.md", directory: false },
		]);
		expect((await hosts.files.list("/workspace", "src"))[0].path).toBe(
			"src/a.ts",
		);
		expect(await hosts.files.list("/workspace", "", true)).toHaveLength(3);
		expect(root[1]).toMatchObject({ size: 42, modifiedAt: 1234 });
		expect(worker.list).toHaveBeenCalledWith("/workspace/src");
		worker.list.mockResolvedValue([]);
		expect(await hosts.files.list("/workspace", "")).toEqual([]);
	});

	it("reads previews through the Worker with absolute AgentFS paths", async () => {
		const { hosts, worker } = fixture();
		expect(await hosts.files.read("/workspace", "src/a.ts")).toBe(
			"actual worker contents",
		);
		expect(worker.read).toHaveBeenCalledWith("/workspace/src/a.ts");
		worker.read.mockRejectedValue(new Error("Worker unavailable"));
		await expect(hosts.files.read("/workspace", "missing")).rejects.toThrow(
			"Worker unavailable",
		);
	});

	it("refuses workspace escape without accessing the Worker", async () => {
		const { hosts, worker } = fixture();
		for (const path of [
			"../secret",
			"/etc/passwd",
			"src/../../secret",
			"a\\b",
		]) {
			await expect(hosts.files.read("/workspace", path)).rejects.toThrow(
				"inside /workspace",
			);
		}
		await expect(hosts.files.list("/other", "")).rejects.toThrow(
			"workspace is /workspace",
		);
		expect(worker.read).not.toHaveBeenCalled();
		expect(worker.list).not.toHaveBeenCalled();
	});

	it("does not fabricate mutations, healthy git, issues, PTYs or action execution", async () => {
		const { hosts } = fixture();
		const operations = [
			() => hosts.files.write("/workspace", "a", "text"),
			() => hosts.files.rename("/workspace", "a", "b"),
			() => hosts.files.createDirectory("/workspace", "a"),
			() => hosts.files.remove("/workspace", "a", true),
			() => hosts.files.copy("/workspace", "a", "b"),
			() => hosts.files.copyToWorkspace("/workspace", "a", "/other", "b"),
			() => hosts.files.move("/workspace", "a", "b"),
			() => hosts.git.status("/workspace"),
			() => hosts.git.diff("/workspace", "a", false),
			() => hosts.git.log("/workspace"),
			() => hosts.git.stage("/workspace", [], true),
			() => hosts.git.commit("/workspace", "message"),
			() => hosts.git.branches.list("/workspace"),
			() => hosts.git.branches.switch("/workspace", "main"),
			() => hosts.git.remote("/workspace", "fetch"),
			() => hosts.issues.list("/workspace"),
			() => hosts.issues.setStatus("/workspace", "x", "closed"),
			() => hosts.terminal.open("/workspace", { cols: 80, rows: 24 }, () => {}),
			() => hosts.actions.invoke("unknown", []),
		];
		for (const operation of operations)
			await expect(operation()).rejects.toThrow("unavailable");
		expect(await hosts.actions.offers("resource-menu", [])).toEqual([]);
		const changed = vi.fn();
		hosts.files.watch("/workspace", changed)();
		expect(changed).not.toHaveBeenCalled();
	});
});
