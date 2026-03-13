import { render, waitFor } from "@testing-library/react";
import { describe, expect, it, vi, beforeEach } from "vitest";

const fetchFileTreeMuxMock = vi.fn();

vi.mock("@/lib/mux-files", () => ({
	fetchFileTreeMux: (...args: unknown[]) => fetchFileTreeMuxMock(...args),
}));

import { FileMentionPopup } from "@/components/chat/file-mention-popup";

describe("FileMentionPopup reliability", () => {
	beforeEach(() => {
		vi.clearAllMocks();
		fetchFileTreeMuxMock.mockResolvedValue([]);
	});

	it("uses bounded tree depth when indexing mentions", async () => {
		render(
			<FileMentionPopup
				query=""
				isOpen
				workspacePath="/tmp/ws"
				onSelect={vi.fn()}
				onClose={vi.fn()}
			/>,
		);

		await waitFor(() => expect(fetchFileTreeMuxMock).toHaveBeenCalledTimes(1));
		expect(fetchFileTreeMuxMock).toHaveBeenCalledWith("/tmp/ws", ".", 6, false);
	});
});
