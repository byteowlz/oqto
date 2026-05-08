import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import type { DisplayMessage, DisplayPart } from "../hooks/types";

type TimelineTreeNode = {
	id: string;
	role: DisplayMessage["role"];
	text: string;
	model?: string | null;
	timestamp: number;
	parentId: string | null;
	children: string[];
	toolLabels: string[];
};

type TimelineTreeViewProps = {
	messages: DisplayMessage[];
	currentHeadId?: string | null;
	onSelectMessage?: (messageId: string) => void;
};

export function TimelineTreeView({
	messages,
	currentHeadId,
	onSelectMessage,
}: TimelineTreeViewProps) {
	const model = buildLinearTimelineModel(messages, currentHeadId ?? null);
	const headId = model.headId;
	const activePath = new Set(headId ? ancestors(model.nodes, headId) : []);

	if (messages.length === 0) {
		return null;
	}

	return (
		<div className="grid h-full min-h-0 grid-cols-[minmax(220px,0.9fr)_minmax(320px,1.1fr)] gap-2">
			<section className="min-h-0 overflow-auto border border-border bg-background/70 p-3">
				<div className="mb-3 flex items-center justify-between gap-2">
					<div>
						<div className="text-xs font-semibold uppercase tracking-[0.18em] text-muted-foreground">
							Timeline graph
						</div>
						<div className="text-xs text-muted-foreground">
							{model.orderedIds.length} turns · head {headId ?? "none"}
						</div>
					</div>
					<Badge variant="outline">preview</Badge>
				</div>
				<div className="relative min-w-max py-4">
					<div className="absolute left-[17px] top-6 bottom-6 w-px bg-border" />
					{model.orderedIds.map((id, index) => {
						const node = model.nodes[id];
						const isHead = id === headId;
						const isActive = activePath.has(id);
						return (
							<button
								key={id}
								type="button"
								className="relative mb-3 flex w-full items-center gap-3 text-left"
								onClick={() => onSelectMessage?.(id)}
							>
								<span
									className={[
										"z-10 grid h-9 w-9 shrink-0 place-items-center border text-[10px] font-semibold",
										node.role === "user" &&
											"bg-primary text-primary-foreground",
										node.role === "assistant" &&
											"bg-background text-foreground",
										node.role === "system" && "bg-muted text-muted-foreground",
										isActive && "ring-2 ring-primary/35",
										isHead && "border-primary",
									]
										.filter(Boolean)
										.join(" ")}
								>
									{index + 1}
								</span>
								<span className="min-w-0 flex-1">
									<span className="flex items-center gap-2 text-xs">
										<span className="font-medium">{node.role}</span>
										{isHead && <Badge className="h-5">head</Badge>}
										{node.children.length > 1 && (
											<Badge variant="secondary" className="h-5">
												{node.children.length} forks
											</Badge>
										)}
									</span>
									<span className="block max-w-[34ch] truncate text-xs text-muted-foreground">
										{node.text || "(empty)"}
									</span>
								</span>
							</button>
						);
					})}
				</div>
			</section>
			<section className="min-h-0 overflow-auto border border-border bg-muted/20 p-3">
				<div className="mb-3 flex items-center justify-between gap-2">
					<div>
						<div className="text-xs font-semibold uppercase tracking-[0.18em] text-muted-foreground">
							Branch transcript
						</div>
						<div className="text-xs text-muted-foreground">
							active path from root to current head
						</div>
					</div>
					<Button size="sm" variant="outline" disabled={!headId}>
						checkout
					</Button>
				</div>
				<div className="space-y-3">
					{model.activeTranscript.map((id) => {
						const node = model.nodes[id];
						return (
							<article
								key={id}
								className="border border-border bg-background p-3 shadow-sm"
							>
								<div className="mb-2 flex items-center gap-2 text-xs text-muted-foreground">
									<Badge variant={node.role === "user" ? "default" : "outline"}>
										{node.role}
									</Badge>
									{node.model && <span>{node.model}</span>}
									<span className="ml-auto">{formatTime(node.timestamp)}</span>
								</div>
								<p className="whitespace-pre-wrap text-sm leading-relaxed">
									{node.text || "(empty message)"}
								</p>
								{node.toolLabels.length > 0 && (
									<div className="mt-2 flex flex-wrap gap-1">
										{node.toolLabels.map((label) => (
											<Badge
												key={label}
												variant="secondary"
												className="font-mono"
											>
												{label}
											</Badge>
										))}
									</div>
								)}
							</article>
						);
					})}
				</div>
			</section>
		</div>
	);
}

function buildLinearTimelineModel(
	messages: DisplayMessage[],
	currentHeadId: string | null,
) {
	const nodes: Record<string, TimelineTreeNode> = {};
	const orderedIds: string[] = [];
	let parentId: string | null = null;
	for (const message of messages) {
		nodes[message.id] = {
			id: message.id,
			role: message.role,
			text: messageText(message.parts),
			model: message.model,
			timestamp: message.timestamp,
			parentId,
			children: [],
			toolLabels: toolLabels(message.parts),
		};
		if (parentId && nodes[parentId]) {
			nodes[parentId].children.push(message.id);
		}
		orderedIds.push(message.id);
		parentId = message.id;
	}
	const fallbackHead = orderedIds.at(-1) ?? null;
	const headId =
		currentHeadId && nodes[currentHeadId] ? currentHeadId : fallbackHead;
	return {
		nodes,
		orderedIds,
		headId,
		activeTranscript: headId ? ancestors(nodes, headId) : [],
	};
}

function ancestors(nodes: Record<string, TimelineTreeNode>, id: string) {
	const out: string[] = [];
	let current: string | null = id;
	while (current && nodes[current]) {
		out.push(current);
		current = nodes[current].parentId;
	}
	return out.reverse();
}

function messageText(parts: DisplayPart[]) {
	return parts
		.map((part) => {
			if (part.type === "text" || part.type === "thinking") return part.text;
			if (part.type === "error" || part.type === "compaction") return part.text;
			if (part.type === "tool_result") return stringifyUnknown(part.output);
			return "";
		})
		.filter(Boolean)
		.join("\n");
}

function toolLabels(parts: DisplayPart[]) {
	return parts
		.map((part) => {
			if (part.type === "tool_call") return `${part.name}:${part.status}`;
			if (part.type === "tool_result") {
				return `${part.name ?? part.toolCallId}:${part.isError ? "error" : "ok"}`;
			}
			return null;
		})
		.filter((label): label is string => Boolean(label));
}

function stringifyUnknown(value: unknown) {
	if (value == null) return "";
	if (typeof value === "string") return value;
	try {
		return JSON.stringify(value);
	} catch {
		return String(value);
	}
}

function formatTime(ts: number) {
	if (!Number.isFinite(ts) || ts <= 0) return "";
	return new Date(ts).toLocaleTimeString([], {
		hour: "2-digit",
		minute: "2-digit",
	});
}
