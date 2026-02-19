/**
 * Test that tool output does NOT leak into chat text for hstry-imported messages.
 *
 * When messages are loaded from hstry, tool result messages (role="tool") have
 * both a "text" part (containing the raw output) and a "tool_result" part in
 * their parts_json. The text part should NOT be rendered as chat text - only
 * the tool_result should be merged into the parent assistant message.
 */
import { normalizeMessages } from "@/features/chat/hooks/message-utils";
import type { RawMessage } from "@/features/chat/hooks/types";
import { describe, expect, it } from "vitest";

describe("tool output leak from hstry imports", () => {
	it("should NOT create text parts from tool role messages with parts_json", () => {
		// Simulate hstry SerializableMessage format (as received via WebSocket)
		const rawMessages: RawMessage[] = [
			{
				role: "user",
				content: "test opus",
				parts_json: JSON.stringify([{ text: "test opus", type: "text" }]),
				created_at_ms: 1000,
			},
			{
				role: "assistant",
				content:
					'I\'ll help you test something related to "opus". Let me first check.',
				parts_json: JSON.stringify([
					{
						text: "The user is saying test opus...",
						type: "thinking",
					},
					{
						text: 'I\'ll help you test something related to "opus". Let me first check.',
						type: "text",
					},
					{
						arguments: { command: "find . -name opus" },
						id: "call_abc123",
						name: "bash",
						type: "toolCall",
					},
				]),
				created_at_ms: 2000,
			},
			{
				// This is the problematic tool result message from hstry
				role: "tool",
				content:
					"./frontend/node_modules/typescript/lib/diagnosticMessages.json\n./deploy/container/skel/.config/opencode/opencode.json",
				parts_json: JSON.stringify([
					{
						// THIS text part contains the raw tool output - should NOT leak
						text: "./frontend/node_modules/typescript/lib/diagnosticMessages.json\n./deploy/container/skel/.config/opencode/opencode.json",
						type: "text",
					},
					{
						is_error: false,
						name: "bash",
						output: [
							{
								text: "./frontend/node_modules/typescript/lib/diagnosticMessages.json\n./deploy/container/skel/.config/opencode/opencode.json",
								type: "text",
							},
						],
						tool_call_id: "call_abc123",
						type: "tool_result",
					},
				]),
				created_at_ms: 3000,
			},
			{
				role: "assistant",
				content: "Let me check the settings.json file.",
				parts_json: JSON.stringify([
					{
						text: "Let me check the settings.json file.",
						type: "text",
					},
					{
						arguments: { command: "cat .pi/settings.json" },
						id: "call_def456",
						name: "bash",
						type: "toolCall",
					},
				]),
				created_at_ms: 4000,
			},
			{
				role: "tool",
				content:
					'{\n  "defaultModel": "claude-opus-4.6",\n  "defaultProvider": "anthropic"\n}',
				parts_json: JSON.stringify([
					{
						text: '{\n  "defaultModel": "claude-opus-4.6",\n  "defaultProvider": "anthropic"\n}',
						type: "text",
					},
					{
						is_error: false,
						name: "bash",
						output: [
							{
								text: '{\n  "defaultModel": "claude-opus-4.6",\n  "defaultProvider": "anthropic"\n}',
								type: "text",
							},
						],
						tool_call_id: "call_def456",
						type: "tool_result",
					},
				]),
				created_at_ms: 5000,
			},
		];

		const displayMessages = normalizeMessages(rawMessages, "test");

		// Tool messages should be merged into assistant messages, not standalone
		const roles = displayMessages.map((m) => m.role);
		expect(roles).not.toContain("tool");

		// Check that no text part contains tool output
		for (const msg of displayMessages) {
			for (const part of msg.parts) {
				if (part.type === "text") {
					// Text parts should NOT contain tool output
					expect(part.text).not.toContain("diagnosticMessages.json");
					expect(part.text).not.toContain("defaultModel");
					expect(part.text).not.toContain("claude-opus-4.6");
				}
			}
		}

		// The tool_result parts should exist in the assistant messages
		const assistantMessages = displayMessages.filter(
			(m) => m.role === "assistant",
		);
		expect(assistantMessages.length).toBe(2);

		// First assistant message should have: thinking, text, tool_call, tool_result
		const firstAssistant = assistantMessages[0];
		const firstPartTypes = firstAssistant.parts.map((p) => p.type);
		expect(firstPartTypes).toContain("thinking");
		expect(firstPartTypes).toContain("text");
		expect(firstPartTypes).toContain("tool_call");
		expect(firstPartTypes).toContain("tool_result");

		// Second assistant message should have: text, tool_call, tool_result
		const secondAssistant = assistantMessages[1];
		const secondPartTypes = secondAssistant.parts.map((p) => p.type);
		expect(secondPartTypes).toContain("text");
		expect(secondPartTypes).toContain("tool_call");
		expect(secondPartTypes).toContain("tool_result");
	});

	it("should correctly normalize the actual hstry test opus session data", () => {
		// This test uses the EXACT data from the hstry database for the "test opus" session
		// that showed the bug in the screenshot
		const rawMessages: RawMessage[] = [
			{
				role: "user",
				content: "test opus",
				parts_json: JSON.stringify([{ text: "test opus", type: "text" }]),
				created_at_ms: 1000,
			},
			{
				role: "assistant",
				content:
					"I'll help you test something related to \"opus\". Let me first check what opus refers to in the codebase to understand what you'd like to test.",
				parts_json: JSON.stringify([
					{
						text: 'The user is saying "test opus". This is a very brief request...',
						type: "thinking",
					},
					{
						text: "I'll help you test something related to \"opus\". Let me first check what opus refers to in the codebase to understand what you'd like to test.",
						type: "text",
					},
					{
						arguments: {
							command:
								'find . -type f \\( -name "*.toml" -o -name "*.json" \\) -exec grep -l -i "opus" {} \\; 2>/dev/null | head -20',
						},
						id: "call_5aa94957d0de4bbdb8fad4e0",
						name: "bash",
						type: "toolCall",
					},
				]),
				created_at_ms: 2000,
			},
			{
				role: "tool",
				content:
					"./frontend/node_modules/typescript/lib/pl/diagnosticMessages.generated.json\n./deploy/container/skel/.config/opencode/opencode.json\n./.pi/settings.json\n",
				parts_json: JSON.stringify([
					{
						text: "./frontend/node_modules/typescript/lib/pl/diagnosticMessages.generated.json\n./deploy/container/skel/.config/opencode/opencode.json\n./.pi/settings.json\n",
						type: "text",
					},
					{
						is_error: false,
						name: "bash",
						output: [
							{
								text: "./frontend/node_modules/typescript/lib/pl/diagnosticMessages.generated.json\n./deploy/container/skel/.config/opencode/opencode.json\n./.pi/settings.json\n",
								type: "text",
							},
						],
						tool_call_id: "call_5aa94957d0de4bbdb8fad4e0",
						type: "tool_result",
					},
				]),
				created_at_ms: 3000,
			},
			{
				role: "assistant",
				content:
					"Let me check these files to see what opus refers to, particularly the settings.json and opencode.json files.",
				parts_json: JSON.stringify([
					{
						text: "Let me check these files to see what opus refers to, particularly the settings.json and opencode.json files.",
						type: "thinking",
					},
					{
						arguments: {
							command:
								'grep -i -A 5 -B 5 "opus" ./.pi/settings.json 2>/dev/null',
						},
						id: "call_98419e782ad54bfa9f40ff3e",
						name: "bash",
						type: "toolCall",
					},
				]),
				created_at_ms: 4000,
			},
			{
				role: "tool",
				content:
					'{\n  "defaultModel": "claude-opus-4.6",\n  "defaultProvider": "anthropic"\n}\n',
				parts_json: JSON.stringify([
					{
						text: '{\n  "defaultModel": "claude-opus-4.6",\n  "defaultProvider": "anthropic"\n}\n',
						type: "text",
					},
					{
						is_error: false,
						name: "bash",
						output: [
							{
								text: '{\n  "defaultModel": "claude-opus-4.6",\n  "defaultProvider": "anthropic"\n}\n',
								type: "text",
							},
						],
						tool_call_id: "call_98419e782ad54bfa9f40ff3e",
						type: "tool_result",
					},
				]),
				created_at_ms: 5000,
			},
		];

		const displayMessages = normalizeMessages(rawMessages, "test");

		// Print the result for debugging
		for (const msg of displayMessages) {
			const partSummary = msg.parts.map((p) => {
				if (p.type === "text") return `text:"${p.text.slice(0, 40)}..."`;
				if (p.type === "thinking")
					return `thinking:"${p.text.slice(0, 40)}..."`;
				if (p.type === "tool_call") return `tool_call:${p.name}`;
				if (p.type === "tool_result") return `tool_result:${p.name}`;
				return p.type;
			});
			console.log(`Message role=${msg.role} parts=[${partSummary.join(", ")}]`);
		}

		// Check that no text contains tool output
		for (const msg of displayMessages) {
			for (const part of msg.parts) {
				if (part.type === "text") {
					expect(part.text).not.toContain("diagnosticMessages");
					expect(part.text).not.toContain("defaultModel");
				}
			}
		}

		// Should have user + 2 assistant messages (tool messages merged in)
		expect(displayMessages.length).toBe(3);
		expect(displayMessages.map((m) => m.role)).toEqual([
			"user",
			"assistant",
			"assistant",
		]);
	});

	it("should NOT create text parts from canonical tool messages with parts array", () => {
		// Simulate canonical Message format (as received via the 'messages' event)
		// This is what the runner sends for live sessions
		const rawMessages: RawMessage[] = [
			{
				id: "msg_user1",
				role: "user",
				parts: [{ type: "text", id: "p1", text: "test opus" }],
				created_at: 1000,
			},
			{
				id: "msg_asst1",
				role: "assistant",
				parts: [
					{
						type: "thinking",
						id: "p2",
						text: "The user is saying test opus...",
					},
					{
						type: "text",
						id: "p3",
						text: 'I\'ll help you test something related to "opus".',
					},
					{
						type: "tool_call",
						id: "p4",
						toolCallId: "call_abc123",
						name: "bash",
						input: { command: "find . -name opus" },
						status: "success",
					},
				],
				created_at: 2000,
			},
			{
				id: "msg_tool1",
				role: "tool",
				// Canonical tool message has BOTH text and tool_result in parts
				parts: [
					{
						type: "text",
						id: "p5",
						text: "./frontend/node_modules/typescript/lib/diagnosticMessages.json\n./deploy/container/skel/.config/opencode/opencode.json",
					},
					{
						type: "tool_result",
						id: "p6",
						toolCallId: "call_abc123",
						name: "bash",
						output:
							"./frontend/node_modules/typescript/lib/diagnosticMessages.json\n./deploy/container/skel/.config/opencode/opencode.json",
						isError: false,
					},
				],
				created_at: 3000,
				tool_call_id: "call_abc123",
				tool_name: "bash",
			},
		];

		const displayMessages = normalizeMessages(rawMessages, "test");

		// Tool messages should be merged into assistant messages
		const roles = displayMessages.map((m) => m.role);
		expect(roles).not.toContain("tool");

		// Check that no text part contains tool output
		for (const msg of displayMessages) {
			for (const part of msg.parts) {
				if (part.type === "text") {
					expect(part.text).not.toContain("diagnosticMessages.json");
				}
			}
		}

		// Should have user + assistant
		expect(displayMessages.length).toBe(2);

		// Assistant message should have tool_result merged in
		const assistant = displayMessages.find((m) => m.role === "assistant");
		const partTypes = assistant?.parts.map((p) => p.type);
		expect(partTypes).toContain("tool_result");
	});

	it("should NOT leak text from tool messages without parts_json and without parts", () => {
		// Simulate a case where tool message only has content (no parts, no parts_json)
		const rawMessages: RawMessage[] = [
			{
				role: "user",
				content: "test opus",
				created_at: 1000,
			},
			{
				role: "assistant",
				content: [
					{ type: "text", text: "I'll help you test." },
					{
						type: "tool_call",
						id: "call_abc123",
						toolCallId: "call_abc123",
						name: "bash",
						input: { command: "find . -name opus" },
					},
				],
				created_at: 2000,
			},
			{
				role: "tool",
				content: "./diagnosticMessages.json\n./opencode.json",
				created_at: 3000,
				toolCallId: "call_abc123",
				toolName: "bash",
			},
		];

		const displayMessages = normalizeMessages(rawMessages, "test");

		// Tool output should NOT appear as text
		for (const msg of displayMessages) {
			for (const part of msg.parts) {
				if (part.type === "text") {
					expect(part.text).not.toContain("diagnosticMessages.json");
				}
			}
		}
	});
});
