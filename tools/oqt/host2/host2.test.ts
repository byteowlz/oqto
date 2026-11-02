import assert from "node:assert/strict";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { after, before, test } from "node:test";
import { WebSocket } from "ws";
import { connectClient, type OqtClient } from "./client.ts";
import {
	Conversation,
	Session,
	Sessions,
	WorkerConversation,
	WorkerSession,
} from "./contract.ts";
import { forkLauncher, type OqtServer, startServer } from "./server.ts";

const resolver = process.env.OQT_SOURCE_RESOLVER;
if (!resolver) throw new Error("OQT_SOURCE_RESOLVER is required");
let directory: string;
let server: OqtServer;
/** Every init the server handed a worker, so tests can replay its token. */
const launched: { session: string; generation: number; token: string }[] = [];

type Entry = {
	kind: string;
	model?: {
		role: string;
		content: string | { type: string; text?: string }[];
	}[];
};
type View = {
	entries: Entry[];
	docs: Record<
		string,
		{ model?: { provider: string; modelId: string }; thinkingLevel?: string }
	>;
};
const asView = (value: unknown) => value as View;
const assistantTexts = (view: unknown) =>
	asView(view).entries.flatMap((entry) =>
		(entry.model ?? []).flatMap((message) =>
			message.role === "assistant" && Array.isArray(message.content)
				? message.content.flatMap((part) =>
						part.type === "text" && part.text ? [part.text] : [],
					)
				: [],
		),
	);
const userTexts = (view: unknown) =>
	asView(view).entries.flatMap((entry) =>
		(entry.model ?? []).flatMap((message) =>
			message.role !== "user"
				? []
				: typeof message.content === "string"
					? [message.content]
					: message.content.flatMap((part) => (part.text ? [part.text] : [])),
		),
	);
const until = async (check: () => boolean, label: string, ms = 15_000) => {
	const end = Date.now() + ms;
	while (!check()) {
		if (Date.now() > end) throw new Error(`timed out: ${label}`);
		await new Promise((resolve) => setTimeout(resolve, 20));
	}
};
async function open(client: OqtClient, session: string) {
	const s = await client.host.get(Session, { session });
	const c = await client.host.get(Conversation, {
		session,
		conversation: s.selected.value,
	});
	if (c.variant !== "participant") throw new Error("expected participant");
	return { s, c };
}
const accepted = (result: { accepted: boolean }) =>
	assert.equal(result.accepted, true, JSON.stringify(result));

before(async () => {
	directory = await mkdtemp(join(tmpdir(), "oqt-host2-"));
	server = await startServer({
		directory,
		launch: (init) => {
			launched.push({
				session: init.session,
				generation: init.generation,
				token: init.token,
			});
			return forkLauncher(
				new URL("./test-worker.ts", import.meta.url).pathname,
				["--import", resolver],
			)(init);
		},
	});
});
after(async () => {
	await server.close();
	await rm(directory, { recursive: true, force: true });
});

test("two clients share one live conversation; prompt reaches the exact conversation", async () => {
	const a = await connectClient(server.socketPath, server.token);
	const b = await connectClient(server.socketPath, server.token);
	try {
		const { s, c: viewA } = await open(a, "S1");
		const viewB = await b.host.get(Conversation, {
			session: "S1",
			conversation: s.selected.value,
		});
		accepted(await viewA.prompt({ text: "run the slow tool" }));
		await until(
			() => assistantTexts(viewB.view.value).includes("slow-tool-finished"),
			"B sees A's completed turn",
		);
		assert.ok(assistantTexts(viewA.view.value).includes("slow-tool-finished"));
		for (const ref of [s, viewA, viewB]) ref.release();
	} finally {
		await a.close();
		await b.close();
	}
});

test("clients cannot reach worker objects directly", async () => {
	const a = await connectClient(server.socketPath, server.token);
	try {
		await assert.rejects(
			a.host.get(WorkerSession, { session: "S1" }),
			/forbidden|refused|not authorized|unavailable/i,
		);
		await assert.rejects(
			a.host.get(WorkerConversation, { session: "S1", conversation: 1 }),
			/forbidden|refused|not authorized|unavailable/i,
		);
	} finally {
		await a.close();
	}
});

test("client detach mid-run does not stop Session work", async () => {
	const a = await connectClient(server.socketPath, server.token);
	const { s, c } = await open(a, "S2");
	const id = s.selected.value;
	const pid = server.worker("S2")?.pid;
	accepted(await c.prompt({ text: "slow, then detach" }));
	await a.close(); // detach while the slow tool runs
	const b = await connectClient(server.socketPath, server.token);
	try {
		const view = await b.host.get(Conversation, {
			session: "S2",
			conversation: id,
		});
		await until(
			() => assistantTexts(view.view.value).includes("slow-tool-finished"),
			"work finished after detach",
		);
		assert.equal(
			server.worker("S2")?.pid,
			pid,
			"same worker process kept running",
		);
		view.release();
	} finally {
		await b.close();
	}
});

test("requestId makes a prompt exactly-once across a retry", async () => {
	const a = await connectClient(server.socketPath, server.token);
	try {
		const { s, c } = await open(a, "S6");
		const first = await c.prompt({ text: "once please", requestId: "req-1" });
		accepted(first);
		await until(
			() => assistantTexts(c.view.value).includes("reply:once please"),
			"first answer",
		);
		const retry = await c.prompt({ text: "once please", requestId: "req-1" });
		accepted(retry);
		assert.equal(
			(retry as { id: string }).id,
			(first as { id: string }).id,
			"retry returns the original submission",
		);
		assert.equal(
			userTexts(c.view.value).filter((text) => text === "once please").length,
			1,
			"asked once",
		);
		s.release();
		c.release();
	} finally {
		await a.close();
	}
});

test("steer and follow-up queue while busy; a queued input can be cancelled", async () => {
	const a = await connectClient(server.socketPath, server.token);
	try {
		const { s, c } = await open(a, "S7");
		accepted(await c.prompt({ text: "slow first" }));
		const busy = await c.prompt({ text: "rejected while busy" });
		assert.equal(busy.accepted, false, "prompt is rejected while busy");
		accepted(await c.steer({ text: "steer this" }));
		const follow = await c.followUp({ text: "follow later" });
		accepted(follow);
		const cancelled = await c.cancelQueued({
			id: (follow as { id: string }).id,
		});
		assert.equal(cancelled.outcome, "cancelled");
		// The steer joins the running work after the tool call, so the model answers it.
		await until(
			() => assistantTexts(c.view.value).includes("reply:steer this"),
			"steer joined the run",
		);
		await new Promise((resolve) => setTimeout(resolve, 500));
		assert.ok(
			!userTexts(c.view.value).includes("follow later"),
			"cancelled follow-up never ran",
		);
		s.release();
		c.release();
	} finally {
		await a.close();
	}
});

test("model and thinking selection go through the conversation", async () => {
	const a = await connectClient(server.socketPath, server.token);
	try {
		const { s, c } = await open(a, "S8");
		assert.ok(
			s.models.value.available.some((model) => model.modelId === "faux-1"),
			"catalog lists faux-1",
		);
		await c.selectModel({ provider: "faux", modelId: "faux-1" });
		const levels = await c.thinkingLevels();
		assert.ok(levels.includes("off"));
		await until(
			() => asView(c.view.value).docs["pi.agent"]?.model?.modelId === "faux-1",
			"agent model",
		);
		await assert.rejects(
			c.selectModel({ provider: "faux", modelId: "no-such-model" }),
			/Unknown model/,
		);
		s.release();
		c.release();
	} finally {
		await a.close();
	}
});

test("fork at an entry creates and selects a branch that keeps the parent history", async () => {
	const a = await connectClient(server.socketPath, server.token);
	try {
		const { s, c } = await open(a, "S9");
		accepted(await c.prompt({ text: "branch base" }));
		await until(
			() => assistantTexts(c.view.value).includes("reply:branch base"),
			"base answered",
		);
		const last = asView(c.view.value).entries.at(-1) as unknown as {
			id: number;
		};
		const forked = await s.fork({
			conversation: s.selected.value,
			at: String(last.id),
		});
		accepted(forked);
		const child = Number((forked as { id: string }).id);
		await until(() => s.selected.value === child, "fork selected");
		assert.ok(
			s.conversations.value.some((entry) => entry.id === child && entry.parent),
			"fork listed with parent",
		);
		const branch = await a.host.get(Conversation, {
			session: "S9",
			conversation: child,
		});
		if (branch.variant !== "participant")
			throw new Error("expected participant");
		assert.ok(
			assistantTexts(branch.view.value).includes("reply:branch base"),
			"branch sees parent history",
		);
		accepted(await branch.prompt({ text: "only on branch" }));
		await until(
			() => assistantTexts(branch.view.value).includes("reply:only on branch"),
			"branch answered",
		);
		assert.ok(
			!assistantTexts(c.view.value).includes("reply:only on branch"),
			"parent unaffected",
		);
		accepted(await s.select({ conversation: s.root.value }));
		await until(() => s.selected.value === s.root.value, "root selected again");
		for (const ref of [s, c, branch]) ref.release();
	} finally {
		await a.close();
	}
});

test("Sessions lists and creates Sessions", async () => {
	const a = await connectClient(server.socketPath, server.token);
	try {
		const sessions = await a.host.get(Sessions, {});
		await until(
			() => sessions.sessions.value.some((entry) => entry.id === "S1"),
			"existing Session listed",
		);
		const { session } = await sessions.create();
		assert.ok(sessions.sessions.value.some((entry) => entry.id === session));
		const { s, c } = await open(a, session);
		accepted(await c.prompt({ text: "new session" }));
		await until(
			() => assistantTexts(c.view.value).includes("reply:new session"),
			"new Session answers",
		);
		for (const ref of [sessions, s, c]) ref.release();
	} finally {
		await a.close();
	}
});

test("an unauthenticated socket is closed and gets no objects", async () => {
	const socket = new WebSocket(`ws+unix://${server.socketPath}:/`, {
		headers: { authorization: "Bearer wrong" },
	});
	const frames: string[] = [];
	socket.on("message", (data) => frames.push(String(data)));
	await new Promise<void>((resolve) => socket.once("close", () => resolve()));
	const bye = frames
		.map((frame) => JSON.parse(frame))
		.find((frame) => frame.t === "bye");
	assert.equal(bye?.reason, "unauthorized");
	assert.ok(
		!frames.some((frame) => JSON.parse(frame).t === "hello"),
		"no protocol hello for unauthenticated peers",
	);
});

test("a restarted worker keeps the Session root and its history", async () => {
	const a = await connectClient(server.socketPath, server.token);
	const { s, c } = await open(a, "S1");
	const id = s.root.value;
	s.release();
	c.release();
	await a.close();
	const before = server.worker("S1");
	await server.stopWorker("S1");
	const b = await connectClient(server.socketPath, server.token);
	try {
		const again = await b.host.get(Session, { session: "S1" });
		assert.equal(again.root.value, id, "stable root conversation id");
		const view = await b.host.get(Conversation, {
			session: "S1",
			conversation: id,
		});
		assert.ok(
			assistantTexts(view.view.value).includes("slow-tool-finished"),
			"history survives restart",
		);
		const after = server.worker("S1");
		assert.notEqual(after?.pid, before?.pid, "new worker process");
		assert.ok(
			(after?.generation ?? 0) > (before?.generation ?? 0),
			"new generation",
		);
		again.release();
		view.release();
	} finally {
		await b.close();
	}
});

const workerHandshake = async (token: string) => {
	const socket = new WebSocket(`ws+unix://${server.workerSocketPath}:/`, {
		headers: { authorization: `Bearer ${token}` },
	});
	return new Promise<number>((resolve) =>
		socket.once("close", (code) => resolve(code)),
	);
};

test("worker socket refuses foreign, replayed and stale generation tokens", async () => {
	assert.equal(
		await workerHandshake("0".repeat(64)),
		1008,
		"foreign token refused",
	);
	const a = await connectClient(server.socketPath, server.token);
	try {
		const first = await a.host.get(Session, { session: "S4" });
		const live = launched.findLast((entry) => entry.session === "S4");
		assert.ok(live && server.worker("S4")?.connected);
		assert.equal(
			await workerHandshake(live.token),
			1008,
			"a live generation's token is single-use",
		);
		first.release();
		await server.stopWorker("S4");
		const second = await a.host.get(Session, { session: "S4" });
		assert.ok((server.worker("S4")?.generation ?? 0) > live.generation);
		assert.equal(
			await workerHandshake(live.token),
			1008,
			"a retired generation's token is refused",
		);
		assert.ok(
			server.worker("S4")?.connected,
			"replays did not disturb the current worker",
		);
		second.release();
	} finally {
		await a.close();
	}
});

test("one generation at a time: the next starts only after the previous exited", async () => {
	const a = await connectClient(server.socketPath, server.token);
	try {
		const first = await a.host.get(Session, { session: "S5" });
		const one = server.worker("S5");
		first.release();
		await server.stopWorker("S5");
		const pid = one?.pid ?? 0;
		assert.throws(
			() => process.kill(pid, 0),
			"previous worker process is gone",
		);
		const second = await a.host.get(Session, { session: "S5" });
		assert.ok((server.worker("S5")?.generation ?? 0) > (one?.generation ?? 0));
		second.release();
	} finally {
		await a.close();
	}
});
