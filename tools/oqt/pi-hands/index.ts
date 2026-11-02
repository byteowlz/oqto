/** Explicit-load pilot for ordinary host Pi. Does not replace or claim to
 * sandbox local Pi tools. Its sole remote tool always uses destination consent.
 * Identity and Runner paths are owner configuration, never model arguments.
 */
import { execFile } from "node:child_process";
import { createHash } from "node:crypto";
import { isAbsolute } from "node:path";
import { Type } from "@earendil-works/pi-ai";
import { defineTool, type ExtensionAPI } from "@earendil-works/pi-coding-agent";

interface Receipt {
	id: string;
	state: string;
	output?: string | null;
	exit_code?: number | null;
	effects_uncertain?: boolean;
}

export default function piHands(pi: ExtensionAPI) {
	const runner = process.env.PI_HANDS_RUNNER;
	const identity = process.env.PI_HANDS_IDENTITY;
	const environment = process.env.PI_HANDS_ENVIRONMENT;
	if (
		!runner ||
		!identity ||
		!environment ||
		!isAbsolute(runner) ||
		!isAbsolute(identity) ||
		!/^[a-z][a-z0-9-]{0,47}$/.test(environment)
	)
		throw new Error(
			"pi-hands requires absolute PI_HANDS_RUNNER and PI_HANDS_IDENTITY, and a named PI_HANDS_ENVIRONMENT",
		);
	const active = new Set<string>();
	const peer = (args: string[], signal?: AbortSignal): Promise<Receipt> =>
		new Promise((resolve, reject) => {
			execFile(
				runner,
				["personal", "peer", ...args, "--identity", identity],
				{ signal, timeout: 20_000, maxBuffer: 256 * 1024 },
				(error, stdout) => {
					if (error)
						return reject(
							new Error(
								"Runner request failed; inspect the same invocation before retrying",
							),
						);
					try {
						resolve(JSON.parse(stdout));
					} catch {
						reject(new Error("Invalid Runner response"));
					}
				},
			);
		});
	const cancel = async (id: string) => {
		try {
			await peer(["cancel", id]);
		} catch {
			/* Destination expiry remains authoritative; cancellation is not confirmed. */
		}
	};
	pi.on("session_shutdown", async () => {
		await Promise.all([...active].map(cancel));
	});
	pi.registerTool(
		defineTool({
			name: "hands_bash",
			label: "Remote hands",
			description: `Run one shell command on environment '${environment}' after that destination's owner approves the exact command and workdir. Network is disabled. No local execution or target fallback. This does not isolate other Pi tools.`,
			parameters: Type.Object({
				environment: Type.String({ description: `Must be '${environment}'` }),
				command: Type.String({
					description: "Exact command for destination-side approval",
				}),
			}),
			async execute(toolCallId, params, signal, onUpdate, ctx) {
				if (params.environment !== environment)
					throw new Error("Unknown environment; no fallback");
				const session = ctx.sessionManager.getSessionId();
				if (!session) throw new Error("A native Session identity is required");
				const key = createHash("sha256")
					.update(JSON.stringify([session, toolCallId]))
					.digest("hex");
				let id: string | undefined;
				let completed = false;
				try {
					let receipt = await peer(
						[
							"request",
							"--request-key",
							key,
							"--session-label",
							session,
							"--command",
							params.command,
						],
						signal,
					);
					id = receipt.id;
					if (!id) throw new Error("Runner returned no invocation identity");
					active.add(id);
					onUpdate?.({
						content: [
							{
								type: "text",
								text: `Environment ${environment}: waiting for destination approval (${id})`,
							},
						],
						details: { environment, invocation: id },
					});
					const deadline = Date.now() + 330_000;
					while (
						receipt.state === "pending" ||
						receipt.state === "approved" ||
						receipt.state === "running"
					) {
						signal?.throwIfAborted();
						if (Date.now() > deadline)
							throw new Error(
								"Destination response deadline; effects may be uncertain",
							);
						if (receipt.state === "approved")
							receipt = await peer(["run", id], signal);
						else {
							await new Promise((resolve) => setTimeout(resolve, 250));
							receipt = await peer(["status", id], signal);
						}
					}
					completed = true;
					if (receipt.state !== "done" || receipt.exit_code !== 0)
						throw new Error(
							`Environment ${environment}, invocation ${id}: ${receipt.state}, exit ${receipt.exit_code ?? "unknown"}; effects uncertain=${receipt.effects_uncertain ?? true}. ${receipt.output ?? "No output delivered"}`,
						);
					return {
						content: [{ type: "text", text: receipt.output ?? "" }],
						details: { environment, invocation: id, receipt },
					};
				} finally {
					if (id) {
						if (!completed) await cancel(id);
						active.delete(id);
					}
				}
			},
		}),
	);
}
