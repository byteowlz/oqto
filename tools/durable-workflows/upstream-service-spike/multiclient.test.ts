import { mkdtemp, rm } from "node:fs/promises";
import { availableParallelism, getPriority } from "node:os";
import { join } from "node:path";
import { BACKGROUND_CONTEXT as context } from "@earendil-works/chord/context";
import { Client } from "@earendil-works/pi-client";
import { createUnixTransportFactory } from "@earendil-works/pi-client/unix";
import { expect, test, vi } from "vitest";
import * as processRuntime from "../src/experimental/process.ts";
import { startServer } from "../src/experimental/server.ts";
import { AgentController } from "../src/experimental/services/agent-controller.ts";
import { SessionManagement } from "../src/experimental/services/sessions.ts";
import { Transcript } from "../src/experimental/services/transcript.ts";
import { createServerServiceBinding, createSessionServiceBinding } from "./experimental-service-binding.ts";
import { configureExperimentalWorkerModel, createExperimentalSessions } from "./experimental-session-support.ts";

// No Oqto authorization, network provider, tools or real user profile are involved.
// Only model responses are synthetic; server/router, workers, SQLite and clients are real.
test("Oqto candidate: two clients, shared worker, reconnect, SIGKILL and durable continuation", async () => {
	expect(availableParallelism()).toBe(1);
	const root = await mkdtemp("/tmp/oqto-multi-");
	const realSpawn = processRuntime.spawnInternalProcess;
	const spawn = vi
		.spyOn(processRuntime, "spawnInternalProcess")
		.mockImplementation((role, args, options) =>
			realSpawn(
				role,
				args,
				role === "session-worker"
					? { ...options, entryUrl: new URL("./fixtures/faux-session-worker.ts", import.meta.url) }
					: options,
			),
		);
	const clients: Client[] = [];
	const bindings: Array<{ dispose(ctx: typeof context): Promise<void> }> = [];
	let runtime: Awaited<ReturnType<typeof startServer>> | undefined;
	try {
		const agentDir = join(root, "profile");
		await configureExperimentalWorkerModel(agentDir);
		vi.stubEnv("PI_CODING_AGENT_DIR", agentDir);
		await createExperimentalSessions(join(agentDir, "experimental", "sessions"), ["shared"], join(root, "workspace"));
		runtime = await startServer({
			provider: "anthropic",
			model: "claude-sonnet-4-5",
			directory: join(root, "server"),
		});
		async function attach() {
			const client = await Client.connect({
				serverId: runtime!.serverId,
				transportFactory: createUnixTransportFactory({ path: runtime!.socketPath }),
			});
			clients.push(client);
			const management = createServerServiceBinding(client, { services: [SessionManagement] });
			bindings.push(management);
			await management.ready(context);
			await management.use(SessionManagement).attach("shared", context);
			const services = createSessionServiceBinding(client, { services: [AgentController, Transcript] });
			bindings.push(services);
			await services.ready(context);
			return { client, services, controller: services.use(AgentController), transcript: services.use(Transcript) };
		}
		async function prompt(peer: Awaited<ReturnType<typeof attach>>, message: string) {
			const result = await peer.controller.prompt({ message, images: null }, context);
			expect(result.accepted).toBe(true);
			if (!result.accepted) throw new Error(result.error.message);
			expect(await peer.controller.waitForPrompt(result.operationId, context)).toEqual({
				status: "done",
				text: "deterministic remote answer",
				reason: null,
			});
		}
		const a = await attach();
		const b = await attach();
		const firstPid = runtime.workerPids.get("shared");
		expect(firstPid).toBeTypeOf("number");
		expect(getPriority(firstPid!)).toBe(19);
		expect([...runtime.workerPids.keys()]).toEqual(["shared"]);
		expect(a.client.attachment!.attachmentId).not.toBe(b.client.attachment!.attachmentId);
		await prompt(a, "first from client A");
		await vi.waitFor(() => {
			expect(a.transcript.state.value?.entries.filter((e) => e.kind === "pi.assistant")).toHaveLength(1);
			expect(b.transcript.state.value).toEqual(a.transcript.state.value);
		});
		const committed = structuredClone(a.transcript.state.value!.entries);
		await b.services.dispose(context);
		await b.client.dispose();
		expect(runtime.workerPids.get("shared")).toBe(firstPid);
		await prompt(a, "second while B detached");
		const b2 = await attach();
		await vi.waitFor(() => {
			expect(b2.transcript.state.value?.entries.filter((e) => e.kind === "pi.assistant")).toHaveLength(2);
			expect(b2.transcript.state.value).toEqual(a.transcript.state.value);
		});
		for (const entry of committed)
			expect(b2.transcript.state.value!.entries.find((candidate) => candidate.id === entry.id)).toEqual(entry);
		const beforeKill = structuredClone(b2.transcript.state.value!.entries);
		process.kill(firstPid!, "SIGKILL");
		await vi.waitFor(() => expect(runtime!.workerPids.has("shared")).toBe(false));
		const c = await attach();
		const replacementPid = runtime.workerPids.get("shared");
		expect(replacementPid).toBeTypeOf("number");
		expect(replacementPid).not.toBe(firstPid);
		await vi.waitFor(() => expect(c.transcript.state.value?.entries).toEqual(beforeKill));
		await prompt(c, "third after killed worker reopened SQLite");
		await vi.waitFor(() =>
			expect(c.transcript.state.value?.entries.filter((e) => e.kind === "pi.assistant")).toHaveLength(3),
		);
		for (const entry of beforeKill)
			expect(c.transcript.state.value!.entries.find((candidate) => candidate.id === entry.id)).toEqual(entry);
		console.log(
			JSON.stringify({
				proof: "real-worker-multiclient",
				serverId: runtime.serverId,
				firstPid,
				replacementPid,
				assistantEntries: 3,
				availableCPUs: availableParallelism(),
				workerNice: getPriority(replacementPid!),
				scopes: {
					upstream: true,
					oqtoAuthorization: false,
					oqtoFrontend: false,
					physicalTui: false,
					inFlightKill: false,
				},
			}),
		);
	} finally {
		// Disposal must not mask a primary failure; release all readers before stopping owner.
		await Promise.allSettled(bindings.map((b) => b.dispose(context)));
		await Promise.allSettled(clients.map((c) => c.dispose()));
		if (runtime) await runtime.close();
		spawn.mockRestore();
		vi.unstubAllEnvs();
		await rm(root, { recursive: true, force: true });
	}
}, 30000);
