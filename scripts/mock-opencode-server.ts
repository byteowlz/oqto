#!/usr/bin/env bun
/**
 * Mock OpenCode server for testing the frontend.
 * Simulates the OpenCode API including permissions, sessions, messages, and SSE events.
 *
 * Usage:
 *   bun run scripts/mock-opencode-server.ts [port]
 *
 * Then either:
 *   1. Point the Octo backend proxy to this server, or
 *   2. Use the test endpoints directly:
 *      - GET /test/trigger-permissions - Creates 4 test permissions
 *      - GET /test/clear-permissions - Clears all pending permissions
 */

const PORT = parseInt(process.argv[2] || "7274", 10);

// In-memory state
const sessions: Map<
	string,
	{
		id: string;
		title: string;
		status: "idle" | "busy";
		messages: Message[];
	}
> = new Map();

const pendingPermissions: Map<string, Permission[]> = new Map();

type Message = {
	id: string;
	role: "user" | "assistant";
	content: string;
	createdAt: number;
};

type Permission = {
	id: string;
	sessionID: string;
	title: string;
	description?: string;
	tool: string;
	input?: Record<string, unknown>;
	risk?: "low" | "medium" | "high";
	time: { created: number };
};

// SSE clients per session
const sseClients: Map<string, Set<ReadableStreamDefaultController>> = new Map();

function broadcast(sessionId: string, event: { type: string; properties: unknown }) {
	const clients = sseClients.get(sessionId);
	if (!clients) return;

	const data = `data: ${JSON.stringify(event)}\n\n`;
	for (const controller of clients) {
		try {
			controller.enqueue(new TextEncoder().encode(data));
		} catch {
			clients.delete(controller);
		}
	}
}

// Create a default session
const defaultSessionId = "ses_mock_001";
sessions.set(defaultSessionId, {
	id: defaultSessionId,
	title: "Mock Test Session",
	status: "idle",
	messages: [
		{
			id: "msg_001",
			role: "user",
			content: "Hello, can you help me test the permission dialogs?",
			createdAt: Date.now() - 60000,
		},
		{
			id: "msg_002",
			role: "assistant",
			content:
				"Of course! I'll simulate some tool calls that require permission. Let me try to run a bash command...",
			createdAt: Date.now() - 30000,
		},
	],
});

// Helper to generate IDs
let idCounter = 100;
const genId = (prefix: string) => `${prefix}_${++idCounter}`;

// CORS headers
const corsHeaders = {
	"Access-Control-Allow-Origin": "*",
	"Access-Control-Allow-Methods": "GET, POST, PUT, PATCH, DELETE, OPTIONS",
	"Access-Control-Allow-Headers": "Content-Type, x-opencode-directory",
	"Access-Control-Allow-Credentials": "true",
};

// Create a permission and broadcast it
function createPermission(
	sessionId: string,
	tool: string,
	title: string,
	description: string,
	input: Record<string, unknown>,
	risk: "low" | "medium" | "high" = "medium"
): Permission {
	const permission: Permission = {
		id: genId("perm"),
		sessionID: sessionId,
		title,
		description,
		tool,
		input,
		risk,
		time: { created: Date.now() },
	};

	if (!pendingPermissions.has(sessionId)) {
		pendingPermissions.set(sessionId, []);
	}
	pendingPermissions.get(sessionId)!.push(permission);

	// Broadcast the permission event
	broadcast(sessionId, {
		type: "permission.updated",
		properties: permission,
	});

	console.log(`[Permission] Created: ${permission.id} (${tool}) for session ${sessionId}`);
	return permission;
}

const server = Bun.serve({
	port: PORT,
	hostname: "0.0.0.0",
	async fetch(req) {
		const url = new URL(req.url);
		const path = url.pathname;
		const method = req.method;

		// Handle CORS preflight
		if (method === "OPTIONS") {
			return new Response(null, { status: 204, headers: corsHeaders });
		}

		console.log(`${method} ${path}`);

		// GET /api/session - List sessions
		if (method === "GET" && path === "/api/session") {
			const sessionList = Array.from(sessions.values()).map((s) => ({
				id: s.id,
				title: s.title,
				time: { created: Date.now() - 3600000 },
			}));
			return Response.json(sessionList, { headers: corsHeaders });
		}

		// POST /api/session - Create session
		if (method === "POST" && path === "/api/session") {
			const id = genId("ses");
			sessions.set(id, {
				id,
				title: "New Session",
				status: "idle",
				messages: [],
			});
			return Response.json({ id }, { headers: corsHeaders });
		}

		// GET /api/session/:id/message - Get messages
		const messageMatch = path.match(/^\/api\/session\/([^/]+)\/message$/);
		if (method === "GET" && messageMatch) {
			const sessionId = messageMatch[1];
			const session = sessions.get(sessionId);
			if (!session) {
				return new Response("Session not found", { status: 404, headers: corsHeaders });
			}

			// Return messages in OpenCode format
			const messages = session.messages.map((m) => ({
				id: m.id,
				role: m.role,
				parts: [{ type: "text", text: m.content }],
				metadata: {
					time: { created: m.createdAt, completed: m.createdAt },
				},
			}));

			return Response.json(messages, { headers: corsHeaders });
		}

		// POST /api/session/:id/prompt_async - Send message (triggers permissions)
		const promptMatch = path.match(/^\/api\/session\/([^/]+)\/prompt_async$/);
		if (method === "POST" && promptMatch) {
			const sessionId = promptMatch[1];
			const session = sessions.get(sessionId);
			if (!session) {
				return new Response("Session not found", { status: 404, headers: corsHeaders });
			}

			const body = (await req.json()) as { message?: string; parts?: unknown[] };
			const userMessage = body.message || "Test message";

			// Add user message
			session.messages.push({
				id: genId("msg"),
				role: "user",
				content: userMessage,
				createdAt: Date.now(),
			});

			// Simulate assistant thinking and requesting permissions
			session.status = "busy";
			broadcast(sessionId, { type: "session.busy", properties: {} });

			// Create various permission requests after a short delay
			setTimeout(() => {
				createPermission(
					sessionId,
					"bash",
					"Execute shell command",
					"Run a command in the terminal",
					{ command: "npm install lodash", timeout: 30000 },
					"high"
				);
			}, 500);

			setTimeout(() => {
				createPermission(
					sessionId,
					"edit",
					"Edit file",
					"Modify src/index.ts",
					{
						filePath: "/home/user/project/src/index.ts",
						oldString: "const x = 1;",
						newString: "const x = 2;",
					},
					"medium"
				);
			}, 1500);

			setTimeout(() => {
				createPermission(
					sessionId,
					"write",
					"Create new file",
					"Create a new configuration file",
					{
						filePath: "/home/user/project/config.json",
						content: '{ "debug": true }',
					},
					"medium"
				);
			}, 2500);

			setTimeout(() => {
				createPermission(
					sessionId,
					"webfetch",
					"Fetch URL",
					"Download content from the web",
					{ url: "https://api.example.com/data", format: "json" },
					"low"
				);
			}, 3500);

			return Response.json({ success: true }, { headers: corsHeaders });
		}

		// GET /api/session/:id/permission - Get pending permissions
		const permGetMatch = path.match(/^\/api\/session\/([^/]+)\/permission$/);
		if (method === "GET" && permGetMatch) {
			const sessionId = permGetMatch[1];
			const perms = pendingPermissions.get(sessionId) || [];
			return Response.json(perms, { headers: corsHeaders });
		}

		// POST /api/session/:id/permission/:permId - Respond to permission
		const permRespondMatch = path.match(/^\/api\/session\/([^/]+)\/permission\/([^/]+)$/);
		if (method === "POST" && permRespondMatch) {
			const sessionId = permRespondMatch[1];
			const permId = permRespondMatch[2];
			const body = (await req.json()) as { response: string };

			console.log(`[Permission] Response for ${permId}: ${body.response}`);

			// Remove from pending
			const perms = pendingPermissions.get(sessionId) || [];
			const idx = perms.findIndex((p) => p.id === permId);
			if (idx !== -1) {
				perms.splice(idx, 1);
			}

			// Broadcast the reply event
			broadcast(sessionId, {
				type: "permission.replied",
				properties: { sessionID: sessionId, permissionID: permId, response: body.response },
			});

			// If approved, simulate adding an assistant message
			if (body.response === "yes" || body.response === "always") {
				const session = sessions.get(sessionId);
				if (session) {
					setTimeout(() => {
						session.messages.push({
							id: genId("msg"),
							role: "assistant",
							content: `Tool executed successfully! (Permission ${permId} was approved)`,
							createdAt: Date.now(),
						});
						broadcast(sessionId, {
							type: "message.updated",
							properties: { sessionID: sessionId },
						});
					}, 500);
				}
			}

			return Response.json({ success: true }, { headers: corsHeaders });
		}

		// GET /api/session/status - Session status (for polling fallback)
		if (method === "GET" && path === "/api/session/status") {
			const status: Record<string, { status: string }> = {};
			for (const [id, session] of sessions) {
				status[id] = { status: session.status };
			}
			return Response.json(status, { headers: corsHeaders });
		}

		// GET /api/event - SSE endpoint
		if (method === "GET" && path === "/api/event") {
			// Use first session for simplicity
			const sessionId = defaultSessionId;

			const stream = new ReadableStream({
				start(controller) {
					if (!sseClients.has(sessionId)) {
						sseClients.set(sessionId, new Set());
					}
					sseClients.get(sessionId)!.add(controller);

					// Send initial connection event
					const connectEvent = `data: ${JSON.stringify({ type: "server.connected", properties: {} })}\n\n`;
					controller.enqueue(new TextEncoder().encode(connectEvent));

					console.log(`[SSE] Client connected for session ${sessionId}`);
				},
				cancel() {
					const clients = sseClients.get(sessionId);
					if (clients) {
						// Controller will be removed on next broadcast attempt
					}
					console.log(`[SSE] Client disconnected`);
				},
			});

			return new Response(stream, {
				headers: {
					...corsHeaders,
					"Content-Type": "text/event-stream",
					"Cache-Control": "no-cache",
					Connection: "keep-alive",
					"X-Accel-Buffering": "no",
				},
			});
		}

		// GET /api/agent - List agents
		if (method === "GET" && path === "/api/agent") {
			return Response.json(
				[
					{ id: "default", name: "Default", description: "Default coding agent" },
					{ id: "build", name: "Build", description: "Build and compile agent" },
				],
				{ headers: corsHeaders }
			);
		}

		// GET /api/config - Get config
		if (method === "GET" && path === "/api/config") {
			return Response.json(
				{ provider: "anthropic", model: "claude-sonnet-4-20250514" },
				{ headers: corsHeaders }
			);
		}

		// GET /api/command - List commands
		if (method === "GET" && path === "/api/command") {
			return Response.json([], { headers: corsHeaders });
		}

		// POST /api/session/:id/abort - Abort session
		const abortMatch = path.match(/^\/api\/session\/([^/]+)\/abort$/);
		if (method === "POST" && abortMatch) {
			const sessionId = abortMatch[1];
			const session = sessions.get(sessionId);
			if (session) {
				session.status = "idle";
				broadcast(sessionId, { type: "session.idle", properties: {} });
			}
			return Response.json({ success: true }, { headers: corsHeaders });
		}

		// TEST ENDPOINTS

		// GET /test/trigger-permissions - Manually trigger test permissions
		if (method === "GET" && path === "/test/trigger-permissions") {
			const sessionId = url.searchParams.get("session") || defaultSessionId;

			// Clear existing permissions first
			pendingPermissions.set(sessionId, []);

			createPermission(
				sessionId,
				"bash",
				"Execute shell command",
				"Run a command in the terminal",
				{ command: "npm install lodash", timeout: 30000 },
				"high"
			);

			createPermission(
				sessionId,
				"edit",
				"Edit file",
				"Modify src/index.ts",
				{
					filePath: "/home/user/project/src/index.ts",
					oldString: "const x = 1;",
					newString: "const x = 2;",
				},
				"medium"
			);

			createPermission(
				sessionId,
				"write",
				"Create new file",
				"Create a new configuration file",
				{
					filePath: "/home/user/project/config.json",
					content: '{ "debug": true }',
				},
				"medium"
			);

			createPermission(
				sessionId,
				"webfetch",
				"Fetch URL",
				"Download content from the web",
				{ url: "https://api.example.com/data", format: "json" },
				"low"
			);

			return Response.json(
				{ success: true, count: 4, sessionId },
				{ headers: corsHeaders }
			);
		}

		// GET /test/clear-permissions - Clear all pending permissions
		if (method === "GET" && path === "/test/clear-permissions") {
			const sessionId = url.searchParams.get("session") || defaultSessionId;
			pendingPermissions.set(sessionId, []);
			return Response.json({ success: true, sessionId }, { headers: corsHeaders });
		}

		// GET /test/add-message - Add a test message
		if (method === "GET" && path === "/test/add-message") {
			const sessionId = url.searchParams.get("session") || defaultSessionId;
			const role = (url.searchParams.get("role") || "assistant") as "user" | "assistant";
			const content = url.searchParams.get("content") || "This is a test message from the mock server.";

			const session = sessions.get(sessionId);
			if (session) {
				session.messages.push({
					id: genId("msg"),
					role,
					content,
					createdAt: Date.now(),
				});
				broadcast(sessionId, {
					type: "message.updated",
					properties: { sessionID: sessionId },
				});
				return Response.json({ success: true, sessionId }, { headers: corsHeaders });
			}
			return new Response("Session not found", { status: 404, headers: corsHeaders });
		}

		// Fallback
		console.log(`[404] ${method} ${path}`);
		return new Response("Not found", { status: 404, headers: corsHeaders });
	},
});

console.log(`
Mock OpenCode server running on http://0.0.0.0:${PORT}

API Endpoints:
  GET  /api/session                         - List sessions
  POST /api/session                         - Create session
  GET  /api/session/:id/message             - Get messages
  POST /api/session/:id/prompt_async        - Send message (triggers test permissions)
  GET  /api/session/:id/permission          - Get pending permissions
  POST /api/session/:id/permission/:permId  - Respond to permission
  GET  /api/event                           - SSE events

Test Endpoints:
  GET  /test/trigger-permissions            - Create 4 test permissions immediately
  GET  /test/clear-permissions              - Clear all pending permissions
  GET  /test/add-message?content=...        - Add a test message

Default session ID: ${defaultSessionId}

To test with the Octo frontend:
  1. Start this mock server:
     bun run scripts/mock-opencode-server.ts

  2. Start the frontend dev server:
     cd frontend && bun dev

  3. Open the frontend with mock parameters:
     http://localhost:5173/?mockOpencode=http://localhost:${PORT}&mockSession=${defaultSessionId}

  4. Trigger test permissions (in another terminal):
     curl http://localhost:${PORT}/test/trigger-permissions

  5. Permission dialogs should appear in the frontend!

Quick API test:
  curl http://localhost:${PORT}/test/trigger-permissions
  curl http://localhost:${PORT}/api/session/${defaultSessionId}/permission
`);
