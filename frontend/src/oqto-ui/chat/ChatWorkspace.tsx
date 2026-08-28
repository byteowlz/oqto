import { useIsMobile } from "@/hooks/use-mobile";
import { useVirtualizer } from "@tanstack/react-virtual";
import { Bot, FileEdit, FileText } from "lucide-react";
import { type ReactNode, useCallback, useLayoutEffect, useRef } from "react";
import { useTranslation } from "react-i18next";
import type {
	ChatMessage,
	OqtoUiPlatform,
	SessionOverview,
	SessionTask,
	UiNavigation,
	WorkArea,
	WorkDirectory,
} from "../platform/contracts";
import { ChatPane } from "./ChatPane";
import { EditorPane, TerminalPane, WorkAreaTabs } from "./WorkAreaPanes";
import { useTimeline } from "./useTimeline";

type ChatWorkspaceProps = {
	platform: Pick<OqtoUiPlatform, "chat" | "id" | "loadMessages">;
	directory: WorkDirectory;
	session: SessionOverview;
	tasks: SessionTask[];
	workArea: WorkArea;
	workAreaTab: string;
	galleryPane: ReactNode;
	onNavigate: (next: UiNavigation) => void;
};

export function ChatWorkspace({
	platform,
	directory,
	session,
	tasks,
	workArea,
	workAreaTab,
	galleryPane,
	onNavigate,
}: ChatWorkspaceProps) {
	const { t } = useTranslation();
	const editorTab = workArea.tabs.find((tab) => tab.id === "editor");
	return (
		<main className="wb-main" aria-label={t("oqtoUi.chat.label")}>
			<div className="wb-chat-card">
				<WorkAreaTabs
					tabs={workArea.tabs}
					activeTab={workAreaTab}
					chatLabel={session.name}
					chatMeta={`${directory.name} [${session.id}] | ${session.updated}`}
					onNavigate={onNavigate}
				/>
				{workAreaTab === "editor" ? (
					<EditorPane
						fileName={editorTab?.fileName ?? ""}
						lines={workArea.editorLines}
					/>
				) : null}
				{workAreaTab === "terminal" ? (
					<TerminalPane lines={workArea.terminalLines} />
				) : null}
				{workAreaTab === "gallery" ? galleryPane : null}
				{workAreaTab !== "editor" &&
				workAreaTab !== "terminal" &&
				workAreaTab !== "gallery" ? (
					<ChatPane
						key={session.id}
						platform={platform}
						agentName={directory.name}
						sessionId={session.id}
						tasks={tasks}
					/>
				) : null}
			</div>
		</main>
	);
}

/* ChatPane lives in ./ChatPane */
