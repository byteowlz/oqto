import {
	Bot,
	Copy,
	FileEdit,
	FileText,
	GitBranch,
	TestTube2,
	User,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import type {
	LabMessage,
	LabNavigation,
	LabSession,
	LabTask,
	LabWorkArea,
	LabWorkDirectory,
} from "../../modules/lab/model";
import { Composer } from "./Composer";
import { TaskProgress } from "./TaskProgress";
import { EditorPane, TerminalPane, WorkAreaTabs } from "./WorkAreaPanes";

type ChatWorkspaceProps = {
	directory: LabWorkDirectory;
	session: LabSession;
	messages: LabMessage[];
	tasks: LabTask[];
	workArea: LabWorkArea;
	workAreaTab: string;
	onNavigate: (next: LabNavigation) => void;
};

type ActivityKind = NonNullable<LabMessage["activity"]>["kind"];

type ToolIconProps = {
	kind: ActivityKind;
};

function ToolIcon({ kind }: ToolIconProps) {
	if (kind === "edit") return <FileEdit aria-hidden="true" />;
	if (kind === "test") return <TestTube2 aria-hidden="true" />;
	return <FileText aria-hidden="true" />;
}

type MessageGroupProps = {
	message: LabMessage;
	agentName: string;
};

function MessageGroup({ message, agentName }: MessageGroupProps) {
	const { t } = useTranslation();
	const isUser = message.author === "user";
	return (
		<article className="wb-msg" data-author={message.author}>
			<header className="wb-msg__header">
				{isUser ? <User aria-hidden="true" /> : <Bot aria-hidden="true" />}
				<span className="wb-msg__name">
					{isUser ? t("workbench.person.name") : agentName}
				</span>
				<span className="wb-msg__spacer" />
				{isUser ? (
					<button
						className="wb-icon-button"
						type="button"
						aria-label={t("workbench.chat.forkHere")}
					>
						<GitBranch aria-hidden="true" />
					</button>
				) : null}
				<span className="wb-msg__time">{message.time}</span>
				<button
					className="wb-icon-button"
					type="button"
					aria-label={t("workbench.chat.copy")}
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
							{t(`workbench.activity.${message.activity.kind}`)}
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
	onNavigate,
}: ChatWorkspaceProps) {
	const { t } = useTranslation();
	const editorTab = workArea.tabs.find((tab) => tab.id === "editor");
	return (
		<main className="wb-main" aria-label={t("workbench.chat.label")}>
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
				{workAreaTab !== "editor" && workAreaTab !== "terminal" ? (
					<ChatPane directory={directory} messages={messages} tasks={tasks} />
				) : null}
			</div>
		</main>
	);
}

type ChatPaneProps = {
	directory: LabWorkDirectory;
	messages: LabMessage[];
	tasks: LabTask[];
};

function ChatPane({ directory, messages, tasks }: ChatPaneProps) {
	const { t } = useTranslation();
	return (
		<>
			<section
				className="wb-chat-panel"
				aria-label={t("workbench.chat.timeline")}
			>
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
