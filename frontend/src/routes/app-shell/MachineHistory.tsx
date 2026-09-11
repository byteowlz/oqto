import {
	Dialog,
	DialogContent,
	DialogDescription,
	DialogHeader,
	DialogTitle,
} from "@/components/ui/dialog";
import { MessageGroupCard } from "@/lib/chat-rendering/CanonicalMessageRenderer";
import { groupMessages } from "@/lib/chat-rendering/group-messages";
import {
	machineChatKey,
	readCachedChat,
	writeCachedChat,
} from "@/lib/machine-chat-cache";
import {
	type MachineHistoryMessage,
	toDisplayMessages,
} from "@/lib/machine-chat-messages";
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
						Stored conversation from this machine. No session is started until
						you send a message.
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
	const cacheKey = machineChatKey(scope, session.workspace ?? "", session.id);
	const page = useQuery({
		queryKey: ["machine-history", scope, session.id, before],
		queryFn: async ({ signal }) => {
			try {
				const fresh = (await port.call(
					{ command: "messages", session_id: session.id, before },
					signal,
				)) as HistoryPage;
				if (!before) {
					void writeCachedChat(
						cacheKey,
						fresh.messages,
						fresh.has_more,
						fresh.next_before,
					);
				}
				return fresh;
			} catch (error) {
				// An unreachable machine must not blank a conversation already read.
				if (!before) {
					const cached = await readCachedChat<MachineHistoryMessage>(cacheKey);
					if (cached) {
						return {
							messages: cached.messages,
							has_more: cached.hasMore,
							next_before: cached.nextBefore ?? undefined,
						} satisfies HistoryPage;
					}
				}
				throw error;
			}
		},
		gcTime: 0,
		staleTime: 0,
		retry: false,
	});
	const messages = page.data?.messages ?? [];
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
			{groupMessages(toDisplayMessages(messages)).map((group, index) => (
				<MessageGroupCard
					key={group.messages[0]?.id ?? index}
					group={group}
					messageId={group.messages[0]?.id}
					workspacePath={session.workspace}
				/>
			))}
			{page.isSuccess && messages.length === 0 && (
				<p>No saved messages in this projection.</p>
			)}
		</section>
	);
}
