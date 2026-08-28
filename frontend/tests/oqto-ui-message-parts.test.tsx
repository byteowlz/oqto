import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { useState } from "react";
import { describe, expect, it, vi } from "vitest";
import { initI18n } from "../lib/i18n";
import { MessageParts } from "../src/oqto-ui/chat/MessageParts";
import {
	type PreviewSelection,
	ResourcePreviewPane,
} from "../src/oqto-ui/chat/ResourcePreviewPane";
import type { ChatMessage } from "../src/oqto-ui/platform/contracts";

initI18n();

const message: ChatMessage = {
	id: "message-1",
	author: "agent",
	content: "",
	time: "now",
	parts: [
		{
			type: "text",
			id: "text-1",
			text: "**Changed** `src/auth.rs:12-18`",
			format: "markdown",
		},
		{
			type: "tool_call",
			id: "call-part",
			toolCallId: "call-1",
			name: "read",
			input: { path: "src/auth.rs" },
			status: "success",
		},
		{
			type: "file_ref",
			id: "file-1",
			uri: "project-context.md",
			label: "project-context.md",
		},
	],
};

function PreviewHarness() {
	const [selection, setSelection] = useState<PreviewSelection | null>(null);
	return selection ? (
		<ResourcePreviewPane
			selection={selection}
			workspacePath="/workspace/project"
			onClose={() => setSelection(null)}
		/>
	) : (
		<MessageParts
			message={message}
			sessionId="session-1"
			resultByCallId={new Map()}
			knownCallIds={new Set(["call-1"])}
			onOpenFile={(path, range) => setSelection({ path, range })}
		/>
	);
}

describe("OqtoUI canonical message rendering", () => {
	it("renders Markdown and tools and activates file resources", () => {
		const onOpenFile = vi.fn();
		render(
			<MessageParts
				message={message}
				sessionId="session-1"
				resultByCallId={new Map()}
				knownCallIds={new Set(["call-1"])}
				onOpenFile={onOpenFile}
			/>,
		);

		expect(screen.getByText("Changed").tagName).toBe("STRONG");
		const inlineReference = screen.getByRole("button", {
			name: "src/auth.rs:12-18",
		});
		expect(screen.getByText("project-context.md")).toBeInTheDocument();

		fireEvent.click(inlineReference);
		expect(onOpenFile).toHaveBeenCalledWith("src/auth.rs", {
			startLine: 12,
			endLine: 18,
		});
		fireEvent.click(screen.getByText("project-context.md"));
		expect(onOpenFile).toHaveBeenLastCalledWith(
			"project-context.md",
			undefined,
		);
	});

	it("reads only after activation and restores the prior surface", async () => {
		const fetchSpy = vi
			.spyOn(globalThis, "fetch")
			.mockResolvedValue(new Response("# Diagram", { status: 200 }));
		try {
			render(<PreviewHarness />);
			expect(fetchSpy).not.toHaveBeenCalled();

			fireEvent.click(screen.getByText("project-context.md"));
			expect(
				screen.getByRole("region", { name: "Preview project-context.md" }),
			).toBeInTheDocument();
			await waitFor(() => expect(fetchSpy).toHaveBeenCalledTimes(1));

			fireEvent.keyDown(document, { key: "Escape" });
			expect(screen.getByText("project-context.md")).toBeInTheDocument();

			fireEvent.click(screen.getByText("project-context.md"));
			fireEvent.click(screen.getByRole("button", { name: "Close preview" }));
			expect(screen.getByText("project-context.md")).toBeInTheDocument();
		} finally {
			fetchSpy.mockRestore();
		}
	});
});
