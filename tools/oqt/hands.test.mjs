import assert from "node:assert/strict";
import {
	copyFile,
	mkdir,
	mkdtemp,
	rm,
	symlink,
	writeFile,
} from "node:fs/promises";
import { createRequire } from "node:module";
import { join } from "node:path";
import { test } from "node:test";
import { pathToFileURL } from "node:url";

const require = createRequire(join(process.env.OQT_PI_SOURCE, "package.json"));
const { BACKGROUND_CONTEXT } = await import(
	pathToFileURL(require.resolve("@earendil-works/chord/context"))
);
const { Connection, RemoteExecutionEnv } = await import(
	pathToFileURL(join(process.env.OQT_PI_SOURCE, "packages/env/src/index.ts"))
);

const launcher = process.env.OQT_HANDS;
const daemon = process.env.OQT_ENV_DAEMON;
const bpf = process.env.OQT_SECCOMP_BPF;
assert.ok(
	launcher && daemon && bpf,
	"explicit confined launcher, daemon and enforcing BPF required",
);

test(
	"real RemoteExecutionEnv: workspace FS/shell work, authority and outside paths remain hidden",
	{ timeout: 15000 },
	async () => {
		const root = await mkdtemp("/var/tmp/oqt-env-proof-");
		const workspace = join(root, "workspace"),
			authority = join(root, "authority"),
			external = join(root, "external");
		await mkdir(workspace);
		await mkdir(authority);
		await mkdir(external);
		await writeFile(join(authority, "credential"), "must-not-disclose");
		await writeFile(join(external, "outside"), "outside-private");
		await symlink(authority, join(workspace, "authority-link"));
		// /tmp is intentionally private inside bwrap; stage the executable in the
		// owner-controlled /var/tmp fixture rather than weakening that mount.
		const stagedDaemon = join(root, "pi-env");
		await copyFile(daemon, stagedDaemon);
		const spec = join(root, "hands.json");
		await writeFile(
			spec,
			JSON.stringify({
				workspace,
				daemon: stagedDaemon,
				bpf,
				protected_roots: [authority],
			}),
			{ mode: 0o600 },
		);
		const logs = [];
		const connection = new Connection({
			command: [launcher, spec],
			onLog: (text) => logs.push(text),
		});
		const env = new RemoteExecutionEnv({
			connection,
			id: "proof-confined-workspace",
			cwd: workspace,
		});
		try {
			const write = await env.writeFile(
				"local.txt",
				"workspace-value",
				BACKGROUND_CONTEXT,
			);
			assert.equal(write.ok, true, JSON.stringify({ write, logs }));
			const read = await env.readTextFile("local.txt", BACKGROUND_CONTEXT);
			assert.equal(read.ok, true);
			assert.equal(read.value, "workspace-value");
			for (const path of [
				join(authority, "credential"),
				join(external, "outside"),
				"authority-link/credential",
			]) {
				const result = await env.readTextFile(path, BACKGROUND_CONTEXT);
				assert.equal(result.ok, false, `${path} unexpectedly readable`);
			}
			let output = "";
			const execution = await env.exec(
				"printf 'shell-value'; cat local.txt; test ! -r ../authority/credential; test ! -r ../external/outside",
				{
					timeout: 3000,
					inheritEnv: false,
					onOutput: (text) => {
						output += text;
					},
				},
				BACKGROUND_CONTEXT,
			);
			assert.equal(execution.ok, true, JSON.stringify({ execution, logs }));
			assert.equal(execution.value.exitCode, 0);
			assert.match(output, /shell-valueworkspace-value/);
			// A shell cannot acquire the owner's environment or broaden filesystem access.
			const leak = await env.exec(
				"cat ../authority/credential",
				{ timeout: 3000, inheritEnv: true },
				BACKGROUND_CONTEXT,
			);
			assert.equal(leak.ok, true);
			assert.notEqual(leak.value.exitCode, 0);
		} finally {
			await env.cleanup(BACKGROUND_CONTEXT);
			connection.close();
			await rm(root, { recursive: true, force: true });
		}
	},
);
