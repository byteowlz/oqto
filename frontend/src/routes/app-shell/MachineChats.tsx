import { useLocalStorage } from "@/hooks/use-local-storage";
import { formatSessionDate } from "@/lib/session-utils";
import { cn } from "@/lib/utils";
import { useQuery } from "@tanstack/react-query";
import {
	ChevronDown,
	ChevronRight,
	FolderKanban,
	KeyRound,
	MessageSquare,
	Plus,
} from "lucide-react";
import { useMemo, useState } from "react";
import {
	type HistoryPort,
	type HistorySession,
	MachineConversation,
} from "./MachineHistory";

export type MachineChatGroup = {
	key: string;
	name: string;
	chats: HistorySession[];
};

/** Groups the flat read-only catalog the way the sessions list groups projects. */
export function groupMachineChats(
	sessions: HistorySession[],
): MachineChatGroup[] {
	const groups = new Map<string, MachineChatGroup>();
	for (const session of sessions) {
		const key = session.workspace?.replace(/\/+$/, "") || "";
		let group = groups.get(key);
		if (!group) {
			group = {
				key,
				name: key ? (key.split("/").pop() ?? key) : "No workspace",
				chats: [],
			};
			groups.set(key, group);
		}
		group.chats.push(session);
	}
	return [...groups.values()].sort((a, b) => b.chats.length - a.chats.length);
}

/**
 * Read-only machine chats presented as the sidebar's own workspace/chat tree.
 * The catalog is only requested once the machine is expanded.
 */
export function MachineChats({
	scope,
	label,
	port,
	online,
	isMobile,
	onNewSession,
	onResumeSession,
	onOpenProviders,
	providersLabel,
}: {
	scope: string;
	label: string;
	port: HistoryPort;
	online: boolean;
	isMobile: boolean;
	/** Only supplied when this machine carries an execution grant. */
	onNewSession?: (directory: string) => void;
	/**
	 * Continue a stored chat on this machine. Only supplied when the machine
	 * is online and carries an execution grant; otherwise a click shows the
	 * read-only snapshot, which can still render from cache while offline.
	 */
	onResumeSession?: (session: HistorySession) => void;
	/** Only supplied when this machine carries a provider-login grant. */
	onOpenProviders?: () => void;
	providersLabel?: string;
}) {
	// Remembered per machine and per account (the scope carries both), so the
	// sidebar reopens exactly as it was left — on a phone especially, where it
	// is dismissed and reopened constantly.
	const [expanded, setExpanded] = useLocalStorage<boolean>(
		`oqto:sidebar:machine:${scope}:expanded`,
		false,
		{ deserialize: (raw) => JSON.parse(raw) === true },
	);
	const [openGroups, setOpenGroups] = useLocalStorage<Set<string>>(
		`oqto:sidebar:machine:${scope}:groups`,
		() => new Set<string>(),
		{
			deserialize: (raw) => {
				const parsed: unknown = JSON.parse(raw);
				return new Set(
					Array.isArray(parsed)
						? parsed.filter((key): key is string => typeof key === "string")
						: [],
				);
			},
			serialize: (value) => JSON.stringify([...value]),
		},
	);
	const [reading, setReading] = useState<HistorySession | null>(null);
	const catalog = useQuery({
		queryKey: ["machine-history", scope, "catalog"],
		queryFn: async ({ signal }) => {
			const data = (await port.call({ command: "list" }, signal)) as {
				sessions: HistorySession[];
			};
			return data.sessions;
		},
		enabled: expanded && online,
		// Dropped as soon as the Account-scoped view unmounts, but kept while it
		// is mounted: rebuilding a whole machine catalog on every expand costs a
		// round trip plus a full scan of that machine's history stores.
		gcTime: 0,
		staleTime: 5 * 60_000,
		retry: false,
	});
	const groups = useMemo(
		() => groupMachineChats(catalog.data ?? []),
		[catalog.data],
	);
	const iconSize = isMobile ? "w-4 h-4" : "w-3 h-3";
	return (
		<>
			<div className="flex items-center justify-between gap-2 py-1.5 px-1 group">
				<button
					type="button"
					onClick={() => setExpanded(!expanded)}
					disabled={!online}
					className="flex flex-1 min-w-0 items-center gap-2 text-left disabled:opacity-40"
					aria-expanded={expanded}
					aria-label={`${label} chat history`}
					title={online ? label : `${label} (offline)`}
				>
					<span className="rounded p-0.5 flex-shrink-0">
						{expanded ? (
							<ChevronDown className={cn("text-muted-foreground", iconSize)} />
						) : (
							<ChevronRight className={cn("text-muted-foreground", iconSize)} />
						)}
					</span>
					<span className="text-muted-foreground text-xs truncate">
						{label}
					</span>
					{catalog.data && (
						<span className="text-muted-foreground/50 text-xs">
							({catalog.data.length})
						</span>
					)}
				</button>
				{onOpenProviders && (
					<button
						type="button"
						onClick={onOpenProviders}
						className="text-muted-foreground hover:text-foreground hover:bg-sidebar-accent rounded p-1 opacity-100 md:opacity-0 md:group-hover:opacity-100 transition-opacity"
						aria-label={providersLabel ?? `${label} providers`}
						title={providersLabel ?? `${label} providers`}
					>
						<KeyRound className={iconSize} />
					</button>
				)}
			</div>
			{expanded && (
				<div className="space-y-1 px-1">
					{catalog.isPending && !catalog.data && (
						<p className="text-muted-foreground/50 text-xs px-1 py-1">
							Loading chats…
						</p>
					)}
					{catalog.isError && (
						<p role="alert" className="text-xs px-1 py-1">
							Machine history unavailable. No other machine will be queried.
						</p>
					)}
					{groups.map((group) => {
						const isOpen = openGroups.has(group.key);
						return (
							<div
								key={group.key}
								className="border-b border-sidebar-border/50 last:border-b-0"
							>
								<div className="flex items-center gap-1 px-1 py-1.5 group">
									<button
										type="button"
										onClick={() =>
											setOpenGroups((current) => {
												const next = new Set(current);
												if (!next.delete(group.key)) next.add(group.key);
												return next;
											})
										}
										className="flex-1 flex items-center gap-1.5 text-left hover:bg-sidebar-accent/50 px-1 py-0.5 -mx-1"
										aria-expanded={isOpen}
									>
										{isOpen ? (
											<ChevronDown
												className={cn(
													"text-muted-foreground flex-shrink-0",
													iconSize,
												)}
											/>
										) : (
											<ChevronRight
												className={cn(
													"text-muted-foreground flex-shrink-0",
													iconSize,
												)}
											/>
										)}
										<FolderKanban
											className={cn(
												"text-primary/70 flex-shrink-0",
												isMobile ? "w-4 h-4" : "w-3.5 h-3.5",
											)}
										/>
										<span
											className={cn(
												"font-medium text-foreground truncate",
												isMobile ? "text-sm" : "text-xs",
											)}
										>
											{group.name}
										</span>
										<span className="text-xs text-muted-foreground">
											({group.chats.length})
										</span>
									</button>
									{onNewSession && group.key && (
										<button
											type="button"
											onClick={() => onNewSession(group.key)}
											className="text-muted-foreground hover:text-primary hover:bg-sidebar-accent rounded p-1 opacity-100 md:opacity-0 md:group-hover:opacity-100 transition-opacity"
											title={`New session in ${group.name} on ${label}`}
											aria-label={`New session in ${group.name} on ${label}`}
										>
											<Plus className={iconSize} />
										</button>
									)}
								</div>
								{isOpen && (
									<div className="space-y-0.5 pb-1">
										{group.chats.map((chat) => (
											<div key={chat.id} className={isMobile ? "pl-4" : "pl-3"}>
												<button
													type="button"
													onClick={() =>
														onResumeSession
															? onResumeSession(chat)
															: setReading(chat)
													}
													className={cn(
														"w-full px-2 text-left transition-colors flex items-start gap-1.5 border border-transparent",
														isMobile ? "py-2" : "py-1",
														reading?.id === chat.id
															? "bg-primary/15 border-primary text-foreground"
															: "text-muted-foreground hover:bg-sidebar-accent",
													)}
												>
													<MessageSquare
														className={cn(
															"mt-0.5 flex-shrink-0 text-primary/70",
															iconSize,
														)}
													/>
													<span className="flex-1 min-w-0">
														<span
															className={cn(
																"block truncate font-medium",
																isMobile ? "text-sm" : "text-xs",
															)}
														>
															{chat.title || "Untitled chat"}
														</span>
														{chat.updated_at && (
															<span
																className={cn(
																	"block text-muted-foreground mt-0.5",
																	isMobile ? "text-[11px]" : "text-[9px]",
																)}
															>
																{formatSessionDate(chat.updated_at)}
															</span>
														)}
													</span>
												</button>
											</div>
										))}
									</div>
								)}
							</div>
						);
					})}
				</div>
			)}
			{reading && (
				<MachineConversation
					key={`${scope}:${reading.id}`}
					scope={scope}
					label={label}
					session={reading}
					port={port}
					close={() => setReading(null)}
				/>
			)}
		</>
	);
}
