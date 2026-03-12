import { Button } from "@/components/ui/button";
import {
	Dialog,
	DialogContent,
	DialogDescription,
	DialogHeader,
	DialogTitle,
} from "@/components/ui/dialog";
import type { ChatSession } from "@/lib/control-plane-client";
import { formatSessionDate, getDisplayPiTitle } from "@/lib/session-utils";
import { cn } from "@/lib/utils";
import { GitBranch } from "lucide-react";
import { useTranslation } from "react-i18next";

export interface BranchGraphDialogProps {
	open: boolean;
	onOpenChange: (open: boolean) => void;
	parentSessions: ChatSession[];
	childSessionsByParent: Map<string, ChatSession[]>;
	selectedSessionId: string | null;
	onSelectSession: (sessionId: string) => void;
}

function BranchNode({
	session,
	childSessionsByParent,
	selectedSessionId,
	onSelectSession,
	depth = 0,
}: {
	session: ChatSession;
	childSessionsByParent: Map<string, ChatSession[]>;
	selectedSessionId: string | null;
	onSelectSession: (sessionId: string) => void;
	depth?: number;
}) {
	const children = childSessionsByParent.get(session.id) || [];
	const isSelected = selectedSessionId === session.id;
	const date = session.updated_at ? formatSessionDate(session.updated_at) : null;

	return (
		<div className="space-y-1">
			<div className="flex items-center gap-2">
				{depth > 0 ? (
					<div className="text-muted-foreground/60 text-xs">{"└".padStart(depth + 1, "─")}</div>
				) : (
					<GitBranch className="w-3.5 h-3.5 text-primary/70" />
				)}
				<button
					type="button"
					onClick={() => onSelectSession(session.id)}
					className={cn(
						"flex-1 rounded border px-2 py-1 text-left text-xs hover:bg-muted/60",
						isSelected
							? "border-primary bg-primary/10 text-foreground"
							: "border-border text-muted-foreground",
					)}
				>
					<div className="font-medium truncate">{getDisplayPiTitle(session)}</div>
					{date && <div className="text-[10px] text-muted-foreground mt-0.5">{date}</div>}
				</button>
			</div>
			{children.length > 0 && (
				<div className="ml-4 border-l border-border pl-2 space-y-1">
					{children.map((child) => (
						<BranchNode
							key={child.id}
							session={child}
							childSessionsByParent={childSessionsByParent}
							selectedSessionId={selectedSessionId}
							onSelectSession={onSelectSession}
							depth={depth + 1}
						/>
					))}
				</div>
			)}
		</div>
	);
}

export function BranchGraphDialog({
	open,
	onOpenChange,
	parentSessions,
	childSessionsByParent,
	selectedSessionId,
	onSelectSession,
}: BranchGraphDialogProps) {
	const { t } = useTranslation();

	return (
		<Dialog open={open} onOpenChange={onOpenChange}>
			<DialogContent className="sm:max-w-3xl max-h-[80vh] overflow-hidden">
				<DialogHeader>
					<DialogTitle>{t("sessions.branchGraph", "Session Branch Graph")}</DialogTitle>
					<DialogDescription>
						{t(
							"sessions.branchGraphDescription",
							"Visual overview of conversation branches. Select a node to switch to that session.",
						)}
					</DialogDescription>
				</DialogHeader>
				<div className="overflow-y-auto max-h-[58vh] space-y-2 pr-1">
					{parentSessions.length === 0 ? (
						<div className="text-sm text-muted-foreground py-4">
							{t("sessions.noSessions")}
						</div>
					) : (
						parentSessions.map((session) => (
							<BranchNode
								key={session.id}
								session={session}
								childSessionsByParent={childSessionsByParent}
								selectedSessionId={selectedSessionId}
								onSelectSession={(id) => {
									onSelectSession(id);
									onOpenChange(false);
								}}
							/>
						))
					)}
				</div>
				<div className="flex justify-end">
					<Button variant="outline" onClick={() => onOpenChange(false)}>
						{t("common.close", "Close")}
					</Button>
				</div>
			</DialogContent>
		</Dialog>
	);
}
