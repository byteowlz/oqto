import { normalizeWorkspaceFileReference } from "@/lib/workspace-resource";
import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";
import type {
	OqtoUiPlatform,
	SessionOverview,
	SessionTask,
	UiNavigation,
	WorkArea,
	WorkDirectory,
} from "../platform/contracts";
import { ChatPane } from "./ChatPane";
import {
	type PreviewSelection,
	ResourcePreviewPane,
} from "./ResourcePreviewPane";
import { EditorPane, TerminalPane, WorkAreaTabs } from "./WorkAreaPanes";

export type ChatWorkspaceContext = {
	directory: WorkDirectory;
	session: SessionOverview;
	tasks: SessionTask[];
};

export type PreviewState = {
	selection: PreviewSelection | null;
	open: (selection: PreviewSelection) => void;
	close: () => void;
};

type ChatWorkspaceProps = {
	platform: Pick<OqtoUiPlatform, "chat" | "id" | "loadMessages">;
	context: ChatWorkspaceContext;
	workArea: WorkArea;
	workAreaTab: string;
	galleryPane: ReactNode;
	previewState: PreviewState;
	onNavigate: (next: UiNavigation) => void;
};

export function ChatWorkspace({
	platform,
	context,
	workArea,
	workAreaTab,
	galleryPane,
	previewState,
	onNavigate,
}: ChatWorkspaceProps) {
	const { directory, session, tasks } = context;
	const {
		selection: preview,
		open: onOpenPreview,
		close: onClosePreview,
	} = previewState;
	const { t } = useTranslation();
	const editorTab = workArea.tabs.find((tab) => tab.id === "editor");
	const readableRef = session.readableId ?? session.id;
	return (
		<main className="wb-main" aria-label={t("oqtoUi.chat.label")}>
			<div className="wb-chat-card">
				<WorkAreaTabs
					tabs={workArea.tabs}
					activeTab={workAreaTab}
					chatLabel={session.name}
					chatMeta={`${directory.name} [${readableRef}] | ${session.updated}`}
					onNavigate={onNavigate}
				/>
				{preview ? (
					<ResourcePreviewPane
						selection={preview}
						workspacePath={directory.path}
						onClose={onClosePreview}
					/>
				) : (
					<>
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
								session={session}
								sessionId={session.id}
								tasks={tasks}
								workspacePath={directory.path}
								onOpenFile={(path, range) => {
									const normalizedPath = normalizeWorkspaceFileReference(
										path,
										directory.path,
									);
									if (normalizedPath)
										onOpenPreview({ path: normalizedPath, range });
								}}
							/>
						) : null}
					</>
				)}
			</div>
		</main>
	);
}
