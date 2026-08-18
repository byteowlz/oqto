import {
	Bot,
	Copy,
	FileEdit,
	FileText,
	GitBranch,
	TestTube2,
	User,
} from "lucide-react";
import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";
import type {
	ChatMessage,
	SessionOverview,
	SessionTask,
	UiNavigation,
	WorkArea,
	WorkDirectory,
} from "../platform/contracts";
import { Composer } from "./Composer";
import { TaskProgress } from "./TaskProgress";
import { EditorPane, TerminalPane, WorkAreaTabs } from "./WorkAreaPanes";

type ChatWorkspaceProps = {
	directory: WorkDirectory;
	session: SessionOverview;
	messages: ChatMessage[];
	tasks: SessionTask[];
	workArea: WorkArea;
	workAreaTab: string;
	galleryPane: ReactNode;
	onNavigate: (next: UiNavigation) => void;
};

type ActivityKind = NonNullable<ChatMessage["activity"]>["kind"];

type ToolIconProps = {
	kind: ActivityKind;
};

function ToolIcon({ kind }: ToolIconProps) {
	if (kind === "edit") return <FileEdit aria-hidden="true" />;
	if (kind === "test") return <TestTube2 aria-hidden="true" />;
	return <FileText aria-hidden="true" />;
}

type MessageGroupProps = {
	message: ChatMessage;
	agentName: string;
};

function MessageGroup({ message, agentName }: MessageGroupProps) {
	const { t } = useTranslation();
	const isUser = message.author === "user";
	const name = isUser
		? t("oqtoUi.person.name")
		: message.author === "tool"
			? t("oqtoUi.chat.tool")
			: agentName;
	return (
		<article className="wb-msg" data-author={message.author}>
			<header className="wb-msg__header">
				{isUser ? <User aria-hidden="true" /> : <Bot aria-hidden="true" />}
				<span className="wb-msg__name">{name}</span>
				<span className="wb-msg__spacer" />
				{isUser ? (
					<button
						className="wb-icon-button"
						type="button"
						aria-label={t("oqtoUi.chat.forkHere")}
					>
						<GitBranch aria-hidden="true" />
					</button>
				) : null}
				<span className="wb-msg__time">{message.time}</span>
				<button
					className="wb-icon-button"
					type="button"
					aria-label={t("oqtoUi.chat.copy")}
				>
					<Copy aria-hidden="true" />
				</button>
			</header>
			<div className="wb-msg__body">
				<p>{message.content}</p>
				{message.activity ? (
					<div className="wb-tool" data-kind={message.activity.kind}>
						<ToolIcon kind={message.activity.kind} />
						<span className="wb-tool__label">
							{t(`oqtoUi.activity.${message.activity.kind}`)}
						</span>
						<span className="wb-tool__target">{message.activity.name}</span>
					</div>
				) : null}
			</div>
		</article>
	);
}

export function ChatWorkspace({
	directory,
	session,
	messages,
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
					<ChatPane directory={directory} messages={messages} tasks={tasks} />
				) : null}
			</div>
		</main>
	);
}

type ChatPaneProps = {
	directory: WorkDirectory;
	messages: ChatMessage[];
	tasks: SessionTask[];
};

function ChatPane({ directory, messages, tasks }: ChatPaneProps) {
	const { t } = useTranslation();
	return (
		<>
			<section className="wb-chat-panel" aria-label={t("oqtoUi.chat.timeline")}>
				{messages.map((message) => (
					<MessageGroup
						agentName={directory.name}
						key={message.id}
						message={message}
					/>
				))}
			</section>

			<TaskProgress tasks={tasks} placement="desktop" />

			<Composer />
		</>
	);
}
