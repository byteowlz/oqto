import { createHash } from "node:crypto";
import { lstat, mkdir } from "node:fs/promises";
import { userInfo } from "node:os";
import { resolve } from "node:path";
import { BACKGROUND_CONTEXT } from "@earendil-works/chord/context";
import type { Models, ModelThinkingLevel } from "@earendil-works/pi-ai";
import {
	createRegistry,
	defineExtension,
	Harness,
	type ModelRef,
} from "@earendil-works/pi-durable";
import { openNodeSqliteStorage } from "@earendil-works/pi-durable/storage/sqlite/node";
import { createCodingTools } from "@earendil-works/pi-durable/tools";
import { CodingAuthority, type CodingRequest } from "./coding-authority.ts";
import { gatedTool } from "./gated-tool.ts";
import { HandsSupervisor } from "./hands-supervisor.ts";

/** Protected local composition. No NodeExecutionEnv, skills, shell config
 * commands, codemode, or unreviewed plugin registry is installed here.
 */
export async function openCodingRuntime(options: {
	databasePath: string;
	directory: string;
	session: string;
	workspace: string;
	launcher: string;
	daemon: string;
	bpf: string;
	protectedRoots: string[];
	models: Models;
	model: ModelRef;
	thinkingLevel?: ModelThinkingLevel;
	policy: "ask" | "deny";
	publish: (requests: CodingRequest[]) => void;
}) {
	const workspace = resolve(options.workspace);
	await mkdir(options.directory, { mode: 0o700, recursive: true });
	const metadata = await lstat(options.directory);
	if (
		!metadata.isDirectory() ||
		metadata.uid !== userInfo().uid ||
		(metadata.mode & 0o077) !== 0
	)
		throw new Error("Coding authority directory must be private and owned");
	const hash = createHash("sha256").update(workspace).digest("hex");
	const supervisor = await HandsSupervisor.open({
		workspace,
		launcher: options.launcher,
		daemon: options.daemon,
		bpf: options.bpf,
		protectedRoots: [...options.protectedRoots, options.directory],
	});
	const authority = new CodingAuthority(
		options.session,
		options.directory,
		{
			account_id: "local-operator",
			principal_id: `uid:${userInfo().uid}`,
			workspace_id: `local:${hash}`,
			work_directory_id: `workdir:${hash}`,
			session_id: options.session,
			audience: "local-operator-only",
		},
		options.publish,
		options.policy,
		async () => {
			await supervisor.replace();
		},
	);
	let harness: Harness | undefined;
	try {
		const registry = createRegistry();
		const coding = createCodingTools();
		registry.install({
			...coding,
			tools: coding.tools?.map((tool) =>
				gatedTool(tool, authority, options.session),
			),
		});
		registry.install(
			defineExtension({
				name: "oqt-protected-coding-prompt",
				sections: [
					{
						key: "oqt",
						render: () =>
							"You are a coding assistant using confined executors. Every coding call needs an exact local-operator approval. Protected runtime, credentials, and authority are unavailable to tools. Never bypass a denial. Interrupted effects are uncertain: inspect before retrying. No skills, codemode, subagents, or unrestricted execution are available.",
					},
				],
			}),
		);
		harness = await Harness.open(
			await openNodeSqliteStorage(options.databasePath),
			{
				models: options.models,
				registry,
				async env(target, context) {
					await authority.ready(context);
					if (!target.cwd || resolve(target.cwd) !== workspace)
						throw new Error(
							"Conversation attempted to change the owner-bound workspace",
						);
					return supervisor.execution;
				},
			},
			BACKGROUND_CONTEXT,
		);
		const conversation = await harness.root(BACKGROUND_CONTEXT, {
			agent: {
				cwd: workspace,
				model: options.model,
				...(options.thinkingLevel
					? { thinkingLevel: options.thinkingLevel }
					: {}),
			},
		});
		return {
			harness,
			conversation,
			authority,
			supervisor,
			async cleanup() {
				try {
					await supervisor.close();
				} finally {
					await authority.close();
				}
			},
		};
	} catch (error) {
		await harness?.close(BACKGROUND_CONTEXT);
		try {
			await supervisor.close();
		} finally {
			await authority.close();
		}
		throw error;
	}
}
