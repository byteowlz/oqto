import {
	Dialog,
	DialogContent,
	DialogDescription,
	DialogHeader,
	DialogTitle,
} from "@/components/ui/dialog";
import { useQuery } from "@tanstack/react-query";
import { useState } from "react";

export type HistorySession = {
	id: string;
	title: string | null;
	workspace: string | null;
	updated_at: string;
};
type HistoryPart = {
	id: string;
	part_type: string;
	text?: string;
	tool_name?: string;
	tool_input?: unknown;
	tool_output?: unknown;
};
type HistoryMessage = { id: string; role: string; parts: HistoryPart[] };
type HistoryPage = {
	messages: HistoryMessage[];
	has_more: boolean;
	next_before?: string;
};
export type HistoryCommand =
	| { command: "list" }
	| { command: "messages"; session_id: string; before: string | null };
export type HistoryPort = {
	call: (command: HistoryCommand, signal: AbortSignal) => Promise<unknown>;
};

/** No execution, file-opening or auth capabilities are supplied to this snapshot view. */
export function MachineConversation({
	scope,
	label,
	session,
	port,
	close,
}: {
	scope: string;
	label: string;
	session: HistorySession;
	port: HistoryPort;
	close: () => void;
}) {
	return (
		<Dialog
			open
			onOpenChange={(open) => {
				if (!open) close();
			}}
		>
			<DialogContent className="flex h-[90dvh] max-w-[min(900px,96vw)] sm:max-w-[min(900px,96vw)] flex-col gap-3 overflow-hidden">
				<DialogHeader>
					<DialogTitle>{label} · Chat history</DialogTitle>
					<DialogDescription>
						Read-only text preview from this machine’s oqto-log. No session is
						started. Tools, files and attachments are not interactive.
					</DialogDescription>
				</DialogHeader>
				<div className="min-h-0 flex-1 overflow-auto">
					<HistoryConversation scope={scope} session={session} port={port} />
				</div>
			</DialogContent>
		</Dialog>
	);
}
function HistoryConversation({
	scope,
	session,
	port,
}: { scope: string; session: HistorySession; port: HistoryPort }) {
	const [before, setBefore] = useState<string | null>(null);
	const page = useQuery({
		queryKey: ["machine-history", scope, session.id, before],
		queryFn: async ({ signal }) =>
			(await port.call(
				{ command: "messages", session_id: session.id, before },
				signal,
			)) as HistoryPage,
		gcTime: 0,
		staleTime: 0,
		retry: false,
	});
	return (
		<section aria-label="Read-only conversation">
			<h2 className="mb-3 font-semibold">{session.title || "Untitled chat"}</h2>
			{page.isPending && <p>Loading messages…</p>}
			{page.isError && (
				<p role="alert">Messages unavailable. The Mac may be offline.</p>
			)}
			<div className="mb-4 flex gap-3 text-sm">
				{page.data?.has_more && page.data.next_before && (
					<button
						type="button"
						className="rounded border px-2 py-1"
						onClick={() => setBefore(page.data?.next_before ?? null)}
					>
						Older messages
					</button>
				)}
				{before && (
					<button
						type="button"
						className="rounded border px-2 py-1"
						onClick={() => setBefore(null)}
					>
						Latest messages
					</button>
				)}
			</div>
			{page.data?.messages.map((message) => (
				<article
					key={message.id}
					className="mb-5 border-b pb-4"
					data-message-id={message.id}
				>
					<h3 className="mb-2 text-xs font-semibold uppercase tracking-wide text-muted-foreground">
						{message.role}
					</h3>
					{message.parts.map((part) => (
						<div key={part.id}>
							{part.text && (
								<div className="whitespace-pre-wrap break-words text-sm leading-relaxed">
									{part.text}
								</div>
							)}
							{part.tool_name && (
								<details className="my-2 rounded border p-2 text-xs">
									<summary>{part.tool_name} · saved tool data</summary>
									<pre className="mt-2 whitespace-pre-wrap break-all">
										{JSON.stringify(
											part.tool_output ?? part.tool_input ?? null,
											null,
											2,
										)?.slice(0, 20000)}
									</pre>
									<p className="text-muted-foreground">
										Preview limited to 20,000 characters.
									</p>
								</details>
							)}
							{!part.text && !part.tool_name && (
								<p className="text-xs text-muted-foreground">
									{part.part_type} · not shown in text preview
								</p>
							)}
						</div>
					))}
				</article>
			))}
			{page.data?.messages.length === 0 && (
				<p>No saved messages in this projection.</p>
			)}
		</section>
	);
}
