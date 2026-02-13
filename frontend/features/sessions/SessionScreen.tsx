"use client";

import { ContextWindowGauge } from "@/components/data-display";
import { ChatSearchBar, ChatView, PiSettingsView } from "@/features/chat";
import { type Features, getFeatures } from "@/features/chat/api";
import {
	type FileTreeState,
	initialFileTreeState,
} from "@/features/sessions/components/FileTreeView";
import { useApp } from "@/hooks/use-app";
import { useIsMobile } from "@/hooks/use-mobile";
import {
	formatTempId,
	getDisplayPiTitle,
	getTempIdFromSession,
	normalizeWorkspacePath,
} from "@/lib/session-utils";
import { cn } from "@/lib/utils";
import {
	Brain,
	CheckSquare,
	ChevronDown,
	ChevronUp,
	CircleDot,
	FileText,
	FolderKanban,
	Globe,
	ListTodo,
	Maximize2,
	MessageSquare,
	Minimize2,
	PaintBucket,
	PanelLeftClose,
	PanelRightClose,
	Plus,
	Search,
	Settings,
	Square,
	Terminal,
	X,
	XCircle,
} from "lucide-react";
import {
	type ComponentType,
	Suspense,
	lazy,
	memo,
	useCallback,
	useEffect,
	useMemo,
	useState,
} from "react";

const BrowserView = lazy(() =>
	import("@/features/sessions/components/BrowserView").then((mod) => ({
		default: mod.BrowserView,
	})),
);
const CanvasView = lazy(() =>
	import("@/features/sessions/components/CanvasView").then((mod) => ({
		default: mod.CanvasView,
	})),
);
const FileTreeView = lazy(() =>
	import("@/features/sessions/components/FileTreeView").then((mod) => ({
		default: mod.FileTreeView,
	})),
);
const MemoriesView = lazy(() =>
	import("@/features/sessions/components/MemoriesView").then((mod) => ({
		default: mod.MemoriesView,
	})),
);
const TerminalView = lazy(() =>
	import("@/features/sessions/components/TerminalView").then((mod) => ({
		default: mod.TerminalView,
	})),
);
const TrxView = lazy(() =>
	import("@/features/sessions/components/TrxView").then((mod) => ({
		default: mod.TrxView,
	})),
);
const WorkspaceOverviewPanel = lazy(() =>
	import("@/features/sessions/components/WorkspaceOverviewPanel").then((mod) => ({
		default: mod.WorkspaceOverviewPanel,
	})),
);
const PreviewView = lazy(() =>
	import("@/features/sessions/components/PreviewView").then((mod) => ({
		default: mod.PreviewView,
	})),
);

type ViewKey =
	| "chat"
	| "overview"
	| "tasks"
	| "files"
	| "canvas"
	| "memories"
	| "terminal"
	| "browser"
	| "settings";

const viewLoadingFallback = (
	<div className="flex-1 flex items-center justify-center text-sm text-muted-foreground">
		Loading...
	</div>
);

type TodoItem = {
	id: string;
	content: string;
	status: "pending" | "in_progress" | "completed" | "cancelled";
	priority: "high" | "medium" | "low";
};

const TodoListView = memo(function TodoListView({
	todos,
	emptyMessage,
	fullHeight = false,
}: {
	todos: TodoItem[];
	emptyMessage: string;
	fullHeight?: boolean;
}) {
	const [isCollapsed, setIsCollapsed] = useState(false);

	const summary = useMemo(() => {
		const pending = todos.filter((t) => t.status === "pending").length;
		const inProgress = todos.filter((t) => t.status === "in_progress").length;
		const completed = todos.filter((t) => t.status === "completed").length;
		const cancelled = todos.filter((t) => t.status === "cancelled").length;
		return { pending, inProgress, completed, cancelled, total: todos.length };
	}, [todos]);

	if (todos.length === 0) {
		return (
			<div
				className={cn(
					"flex items-center justify-center text-sm text-muted-foreground",
					fullHeight && "h-full",
				)}
			>
				{emptyMessage}
			</div>
		);
	}

	if (isCollapsed) {
		return (
			<button
				type="button"
				onClick={() => setIsCollapsed(false)}
				className="flex-shrink-0 w-full flex items-center justify-between px-3 py-2 border-b border-border bg-muted/30 hover:bg-muted/50 transition-colors"
			>
				<div className="flex items-center gap-3 text-[11px] text-muted-foreground">
					<ListTodo className="w-3.5 h-3.5" />
					<span className="font-medium">Tasks</span>
					<span>{summary.total} total</span>
					{summary.inProgress > 0 && (
						<span className="flex items-center gap-1 text-primary">
							<CircleDot className="w-3 h-3" />
							{summary.inProgress}
						</span>
					)}
					{summary.pending > 0 && (
						<span className="flex items-center gap-1 text-muted-foreground">
							<Square className="w-3 h-3" />
							{summary.pending}
						</span>
					)}
				</div>
				<ChevronUp className="w-3.5 h-3.5 text-muted-foreground" />
			</button>
		);
	}

	return (
		<div
			className={cn(
				"flex flex-col flex-shrink-0 overflow-hidden",
				fullHeight ? "h-full" : "max-h-[40%]",
			)}
		>
			{/* Summary header */}
			<div className="px-3 py-2 border-b border-border bg-muted/30">
				<div className="flex items-center justify-between text-xs">
					<span className="text-muted-foreground">{summary.total} tasks</span>
					<div className="flex items-center gap-3">
						{summary.inProgress > 0 && (
							<span className="flex items-center gap-1 text-primary">
								<CircleDot className="w-3 h-3" />
								{summary.inProgress}
							</span>
						)}
						{summary.pending > 0 && (
							<span className="flex items-center gap-1 text-muted-foreground">
								<Square className="w-3 h-3" />
								{summary.pending}
							</span>
						)}
						{summary.completed > 0 && (
							<span className="flex items-center gap-1 text-primary">
								<CheckSquare className="w-3 h-3" />
								{summary.completed}
							</span>
						)}
						<button
							type="button"
							onClick={() => setIsCollapsed(true)}
							className="p-1 hover:bg-muted rounded transition-colors"
							title="Collapse"
						>
							<ChevronDown className="w-3 h-3 text-muted-foreground" />
						</button>
					</div>
				</div>
			</div>

			{/* Todo list */}
			<div className="flex-1 overflow-y-auto px-3 py-2 space-y-1">
				{todos.map((todo, idx) => (
					<div
						key={todo.id || idx}
						className={cn(
							"flex items-start gap-2 p-2 transition-colors",
							todo.status === "in_progress" &&
								"bg-primary/10 border border-primary/30",
							todo.status === "completed" && "opacity-50",
							todo.status === "cancelled" && "opacity-40",
							todo.status === "pending" && "bg-muted/30 border border-border",
						)}
					>
						{/* Status icon */}
						<div className="flex-shrink-0 mt-0.5">
							{todo.status === "completed" ? (
								<CheckSquare className="w-4 h-4 text-primary" />
							) : todo.status === "in_progress" ? (
								<CircleDot className="w-4 h-4 text-primary animate-pulse" />
							) : todo.status === "cancelled" ? (
								<XCircle className="w-4 h-4 text-muted-foreground" />
							) : (
								<Square className="w-4 h-4 text-muted-foreground" />
							)}
						</div>

						{/* Content */}
						<div className="flex-1 min-w-0">
							<p
								className={cn(
									"text-sm leading-relaxed",
									todo.status === "completed"
										? "text-muted-foreground line-through"
										: "text-foreground",
									todo.status === "cancelled" && "line-through",
								)}
							>
								{todo.content}
							</p>
						</div>

						{/* Priority badge */}
						{todo.priority && (
							<span
								className={cn(
									"text-[10px] uppercase tracking-wide flex-shrink-0 px-1.5 py-0.5",
									todo.priority === "high" && "bg-red-400/10 text-red-400",
									todo.priority === "medium" &&
										"bg-yellow-400/10 text-yellow-400",
									todo.priority === "low" && "bg-muted text-muted-foreground",
								)}
							>
								{todo.priority}
							</span>
						)}
					</div>
				))}
			</div>
		</div>
	);
});

const TabButton = memo(function TabButton({
	activeView,
	onSelect,
	view,
	icon: Icon,
	label,
	badge,
	hideLabel,
}: {
	activeView: ViewKey;
	onSelect: (view: ViewKey) => void;
	view: ViewKey;
	icon: ComponentType<{ className?: string }>;
	label: string;
	badge?: number;
	hideLabel?: boolean;
}) {
	return (
		<button
			type="button"
			onClick={() => onSelect(view)}
			className={cn(
				"flex-1 flex items-center justify-center px-1.5 py-1 relative transition-colors",
				activeView === view
					? "bg-primary/15 text-foreground border border-primary"
					: "text-muted-foreground border border-transparent hover:border-border hover:bg-muted/50",
			)}
			title={label}
		>
			<Icon className="w-4 h-4" />
			{!hideLabel && (
				<span className="hidden sm:inline ml-1 text-xs">{label}</span>
			)}
			{badge !== undefined && badge > 0 && (
				<span className="absolute -top-0.5 -right-0.5 w-4 h-4 bg-pink-500 text-white text-[10px] rounded-[2px] flex items-center justify-center border-2 border-background">
					{badge}
				</span>
			)}
		</button>
	);
});

const CollapsedTabButton = memo(function CollapsedTabButton({
	activeView,
	onSelect,
	view,
	icon: Icon,
	label,
	badge,
}: {
	activeView: ViewKey;
	onSelect: (view: ViewKey) => void;
	view: ViewKey;
	icon: ComponentType<{ className?: string }>;
	label: string;
	badge?: number;
}) {
	return (
		<button
			type="button"
			onClick={() => onSelect(view)}
			className={cn(
				"w-8 h-8 flex items-center justify-center relative transition-colors rounded",
				activeView === view
					? "bg-primary/15 text-foreground border border-primary"
					: "text-muted-foreground border border-transparent hover:border-border hover:bg-muted/50",
			)}
			title={label}
		>
			<Icon className="w-4 h-4" />
			{badge !== undefined && badge > 0 && (
				<span className="absolute -top-0.5 -right-0.5 w-4 h-4 bg-pink-500 text-white text-[10px] rounded-[2px] flex items-center justify-center border-2 border-background">
					{badge}
				</span>
			)}
		</button>
	);
});

const EmptyWorkspacePanel = memo(function EmptyWorkspacePanel({
	label,
}: {
	label: string;
}) {
	return (
		<div className="h-full flex items-center justify-center text-sm text-muted-foreground">
			{label}
		</div>
	);
});

export const SessionScreen = memo(function SessionScreen() {
	const {
		locale,
		chatHistory,
		selectedChatSessionId,
		setSelectedChatSessionId,
		selectedChatFromHistory,
		selectedWorkspaceOverviewPath,
		setSelectedWorkspaceOverviewPath,
		createNewChat,
		replaceOptimisticChatSession,
		clearOptimisticChatSession,
		refreshChatHistory,
		scrollToMessageId,
		setScrollToMessageId,
	} = useApp();
	const isMobileLayout = useIsMobile();
	const [features, setFeatures] = useState<Features>({ mmry_enabled: false });
	const [featuresLoaded, setFeaturesLoaded] = useState(false);
	const [activeView, setActiveView] = useState<ViewKey>("chat");
	const [tasksSubTab, setTasksSubTab] = useState<"todos" | "planner">("todos");
	const [latestTodos, setLatestTodos] = useState<TodoItem[]>([]);
	const [rightSidebarCollapsed, setRightSidebarCollapsed] = useState(false);
	const [isSearchOpen, setIsSearchOpen] = useState(false);
	const [tokenUsage, setTokenUsage] = useState({
		inputTokens: 0,
		outputTokens: 0,
		maxTokens: 0,
	});
	const [expandedView, setExpandedView] = useState<ViewKey | null>(null);
	const [previewFilePath, setPreviewFilePath] = useState<string | null>(null);
	const [pendingFileAttachment, setPendingFileAttachment] = useState<{
		id: string;
		path: string;
		filename: string;
		type: "file";
	} | null>(null);
	const [fileTreeState, setFileTreeState] = useState<FileTreeState>(initialFileTreeState);
	const [pendingChatInput, setPendingChatInput] = useState<string | null>(null);

	const handlePreviewFile = useCallback((filePath: string) => {
		setPreviewFilePath(filePath);
	}, []);

	const handleClosePreview = useCallback(() => {
		setPreviewFilePath(null);
	}, []);

	const handleSendToChat = useCallback((text: string) => {
		setPendingChatInput(text);
		setActiveView("chat");
	}, []);

	const handleOpenInCanvas = useCallback((filePath: string) => {
		setActiveView("canvas");
	}, []);

	const handleCanvasSaveAndAddToChat = useCallback((filePath: string) => {
		const filename = filePath.split("/").pop() ?? filePath;
		setPendingFileAttachment({
			id: `canvas-${Date.now()}`,
			path: filePath,
			filename,
			type: "file",
		});
	}, []);

	const handlePendingFileAttachmentConsumed = useCallback(() => {
		setPendingFileAttachment(null);
	}, []);

	const normalizedWorkspacePath = useMemo(
		() => normalizeWorkspacePath(selectedChatFromHistory?.workspace_path),
		[selectedChatFromHistory?.workspace_path],
	);

	const normalizedOverviewPath = useMemo(
		() =>
			selectedWorkspaceOverviewPath
				? normalizeWorkspacePath(selectedWorkspaceOverviewPath)
				: null,
		[selectedWorkspaceOverviewPath],
	);

	const handleNewChat = useCallback(async () => {
		const targetWorkspace = normalizedOverviewPath ?? normalizedWorkspacePath;
		setSelectedWorkspaceOverviewPath(null);
		const id = await createNewChat(targetWorkspace ?? undefined);
		if (id) setSelectedChatSessionId(id);
	}, [
		createNewChat,
		normalizedOverviewPath,
		normalizedWorkspacePath,
		setSelectedChatSessionId,
		setSelectedWorkspaceOverviewPath,
	]);

	useEffect(() => {
		let mounted = true;
		getFeatures()
			.then((data) => {
				if (!mounted) return;
				setFeatures(data);
				setFeaturesLoaded(true);
			})
			.catch(() => {
				if (!mounted) return;
				setFeaturesLoaded(false);
			});
		return () => {
			mounted = false;
		};
	}, []);

	useEffect(() => {
		if (!isMobileLayout) return;
		if (normalizedOverviewPath) {
			setActiveView("overview");
			return;
		}
		if (activeView === "overview") {
			setActiveView("chat");
		}
	}, [activeView, isMobileLayout, normalizedOverviewPath]);

	// Clear file preview and tree state when session changes
	// biome-ignore lint/correctness/useExhaustiveDependencies: intentionally reset on session change
	useEffect(() => {
		setPreviewFilePath(null);
		setFileTreeState(initialFileTreeState);
	}, [selectedChatSessionId]);

	const headerTitle = selectedChatFromHistory
		? getDisplayPiTitle(selectedChatFromHistory)
		: locale === "de"
			? "Chat"
			: "Chat";
	const tempId = selectedChatFromHistory
		? getTempIdFromSession(selectedChatFromHistory)
		: null;
	const tempIdLabel = formatTempId(tempId);
	const workspaceName =
		normalizedWorkspacePath?.split("/").filter(Boolean).pop() ?? null;
	const formattedDate = selectedChatFromHistory?.created_at
		? new Date(selectedChatFromHistory.created_at).toLocaleDateString(
				locale === "de" ? "de-DE" : "en-US",
			)
		: null;
	const overviewWorkspaceName =
		normalizedOverviewPath?.split("/").filter(Boolean).pop() ?? null;
	const isOverviewActive = Boolean(normalizedOverviewPath);

	const chatPanel = normalizedWorkspacePath ? (
		<ChatView
			locale={locale}
			className="flex-1"
			features={features}
			workspacePath={normalizedWorkspacePath}
			selectedSessionId={selectedChatSessionId}
			onSelectedSessionIdChange={setSelectedChatSessionId}
			scrollToMessageId={scrollToMessageId}
			onScrollToMessageComplete={() => setScrollToMessageId(null)}
			onTokenUsageChange={setTokenUsage}
			onTodosChange={setLatestTodos}
			onMessageSent={refreshChatHistory}
			onMessageComplete={refreshChatHistory}
			hideHeader
			pendingFileAttachment={pendingFileAttachment}
			onPendingFileAttachmentConsumed={handlePendingFileAttachmentConsumed}
			pendingChatInput={pendingChatInput}
			onPendingChatInputConsumed={() => setPendingChatInput(null)}
		/>
	) : (
		<EmptyWorkspacePanel
			label={locale === "de" ? "Lade Chat..." : "Loading chat..."}
		/>
	);

	const overviewPanel = normalizedOverviewPath ? (
		<Suspense fallback={viewLoadingFallback}>
			<WorkspaceOverviewPanel
				workspacePath={normalizedOverviewPath}
				locale={locale}
				onClose={() => setSelectedWorkspaceOverviewPath(null)}
			/>
		</Suspense>
	) : null;

	const tasksPanel = (
		<div className="flex flex-col h-full overflow-hidden">
			<div className="flex-shrink-0 flex border-b border-border bg-muted/30">
				<button
					type="button"
					onClick={() => setTasksSubTab("todos")}
					className={cn(
						"flex-1 px-3 py-2 text-xs font-medium transition-colors",
						tasksSubTab === "todos"
							? "text-foreground border-b-2 border-primary bg-background"
							: "text-muted-foreground hover:text-foreground hover:bg-muted/50",
					)}
				>
					<div className="flex items-center justify-center gap-1.5">
						<ListTodo className="w-3.5 h-3.5" />
						<span>Todos</span>
						{latestTodos.length > 0 && (
							<span className="text-[10px] px-1.5 py-0.5 bg-muted rounded-full">
								{latestTodos.length}
							</span>
						)}
					</div>
				</button>
				<button
					type="button"
					onClick={() => setTasksSubTab("planner")}
					className={cn(
						"flex-1 px-3 py-2 text-xs font-medium transition-colors",
						tasksSubTab === "planner"
							? "text-foreground border-b-2 border-primary bg-background"
							: "text-muted-foreground hover:text-foreground hover:bg-muted/50",
					)}
				>
					<div className="flex items-center justify-center gap-1.5">
						<CircleDot className="w-3.5 h-3.5" />
						<span>Planner</span>
					</div>
				</button>
			</div>
			{tasksSubTab === "todos" && (
				<div className="flex-1 min-h-0 overflow-hidden">
					<TodoListView
						todos={latestTodos}
						emptyMessage={locale === "de" ? "Keine Aufgaben" : "No tasks"}
						fullHeight
					/>
				</div>
			)}
			{tasksSubTab === "planner" && (
				<div className="flex-1 min-h-0">
					{normalizedWorkspacePath ? (
						<Suspense fallback={viewLoadingFallback}>
							<TrxView
								key={normalizedWorkspacePath}
								workspacePath={normalizedWorkspacePath}
								className="flex-1 min-h-0"
							/>
						</Suspense>
					) : (
						<EmptyWorkspacePanel
							label={
								locale === "de"
									? "Kein Arbeitsbereich für Planner"
									: "No workspace for planner"
							}
						/>
					)}
				</div>
			)}
		</div>
	);

	const chatHeader = (
		<div className="pb-3 mb-3 border-b border-border pr-20">
			<div className="min-w-0">
				<div className="flex items-center gap-2">
					<h1 className="text-base sm:text-lg font-semibold text-foreground tracking-wider truncate">
						{headerTitle}
					</h1>
				</div>
				<div className="flex items-center gap-2 text-xs text-foreground/60 dark:text-muted-foreground">
					{workspaceName && (
						<span className="font-mono truncate">
							{workspaceName}
							{tempIdLabel && ` [${tempIdLabel}]`}
						</span>
					)}
					{workspaceName && tempIdLabel && formattedDate && (
						<span className="opacity-50">|</span>
					)}
					{formattedDate && (
						<span className="flex-shrink-0">{formattedDate}</span>
					)}
				</div>
			</div>
			<div className="mt-2">
				<ContextWindowGauge
					inputTokens={tokenUsage.inputTokens}
					outputTokens={tokenUsage.outputTokens}
					maxTokens={tokenUsage.maxTokens}
					locale={locale}
					compact
				/>
			</div>
		</div>
	);

	const overviewHeader = (
		<div className="pb-3 mb-3 border-b border-border pr-20">
			<div className="min-w-0">
				<div className="flex items-center gap-2">
					<h1 className="text-base sm:text-lg font-semibold text-foreground tracking-wider truncate">
						{locale === "de" ? "Workspace-Ubersicht" : "Workspace overview"}
					</h1>
				</div>
				<div className="flex items-center gap-2 text-xs text-foreground/60 dark:text-muted-foreground">
					{overviewWorkspaceName && (
						<span className="font-mono truncate">{overviewWorkspaceName}</span>
					)}
					{normalizedOverviewPath && !overviewWorkspaceName && (
						<span className="font-mono truncate">{normalizedOverviewPath}</span>
					)}
				</div>
			</div>
		</div>
	);

	const sessionHeader = isOverviewActive ? overviewHeader : chatHeader;

	const showEmptyChat =
		!isOverviewActive &&
		!selectedChatSessionId &&
		chatHistory.length === 0 &&
		featuresLoaded;

	return (
		<div className="flex flex-col h-full min-h-0 p-1 sm:p-4 md:p-6 gap-1 sm:gap-4">
			{/* Mobile layout */}
			{isMobileLayout && (
				<div className="flex-1 min-h-0 flex flex-col lg:hidden">
					<div className="sticky top-0 z-10 bg-card border border-border rounded-t-xl overflow-hidden">
						<div className="flex gap-0.5 p-1 sm:p-2">
							<TabButton
								activeView={activeView}
								onSelect={setActiveView}
								view="chat"
								icon={MessageSquare}
								label={locale === "de" ? "Chat" : "Chat"}
							/>
							{normalizedOverviewPath && (
								<TabButton
									activeView={activeView}
									onSelect={setActiveView}
									view="overview"
									icon={FolderKanban}
									label={locale === "de" ? "Ubersicht" : "Overview"}
								/>
							)}
							<TabButton
								activeView={activeView}
								onSelect={setActiveView}
								view="tasks"
								icon={ListTodo}
								label={locale === "de" ? "Aufgaben" : "Tasks"}
								badge={latestTodos.length}
							/>
							<TabButton
								activeView={activeView}
								onSelect={setActiveView}
								view="files"
								icon={FileText}
								label={locale === "de" ? "Dateien" : "Files"}
							/>
							<TabButton
								activeView={activeView}
								onSelect={setActiveView}
								view="canvas"
								icon={PaintBucket}
								label="Canvas"
							/>
							{features.mmry_enabled && (
								<TabButton
									activeView={activeView}
									onSelect={setActiveView}
									view="memories"
									icon={Brain}
									label={locale === "de" ? "Erinnerungen" : "Memories"}
								/>
							)}
							<TabButton
								activeView={activeView}
								onSelect={setActiveView}
								view="terminal"
								icon={Terminal}
								label={locale === "de" ? "Terminal" : "Terminal"}
							/>
							<TabButton
								activeView={activeView}
								onSelect={setActiveView}
								view="browser"
								icon={Globe}
								label="Browser"
							/>
							<TabButton
								activeView={activeView}
								onSelect={setActiveView}
								view="settings"
								icon={Settings}
								label={locale === "de" ? "Einstellungen" : "Settings"}
							/>
						</div>
						<ContextWindowGauge
							inputTokens={tokenUsage.inputTokens}
							outputTokens={tokenUsage.outputTokens}
							maxTokens={tokenUsage.maxTokens}
							locale={locale}
							compact
						/>
					</div>

					<div
						className={cn(
							"flex-1 min-h-0 bg-card border border-t-0 border-border rounded-b-xl p-1.5 sm:p-4 overflow-hidden flex flex-col",
							activeView === "chat" && "pb-0",
						)}
					>
						{activeView === "chat" && chatPanel}
									{activeView === "overview" && overviewPanel}
						<div
							className={cn(
								"h-full flex flex-col",
								activeView !== "files" && "hidden",
							)}
						>
							{previewFilePath ? (
								<Suspense fallback={viewLoadingFallback}>
									<PreviewView
										filePath={previewFilePath}
										workspacePath={normalizedWorkspacePath}
										onClose={handleClosePreview}
										showHeader
									/>
								</Suspense>
							) : normalizedWorkspacePath ? (
								<Suspense fallback={viewLoadingFallback}>
									<FileTreeView
										workspacePath={normalizedWorkspacePath}
										onPreviewFile={handlePreviewFile}
										onOpenInCanvas={handleOpenInCanvas}
										state={fileTreeState}
										onStateChange={setFileTreeState}
									/>
								</Suspense>
							) : (
								<EmptyWorkspacePanel
									label={
										locale === "de"
											? "Kein Arbeitsbereich für Dateien"
											: "No workspace for files"
									}
								/>
							)}
						</div>
						{activeView === "tasks" && tasksPanel}
						{activeView === "canvas" && (
							<div className="flex-1 min-h-0">
								<Suspense fallback={viewLoadingFallback}>
									<CanvasView workspacePath={normalizedWorkspacePath} onSaveAndAddToChat={handleCanvasSaveAndAddToChat} />
								</Suspense>
							</div>
						)}
						{features.mmry_enabled && activeView === "memories" && (
							<Suspense fallback={viewLoadingFallback}>
								<MemoriesView
									workspacePath={normalizedWorkspacePath}
									storeName={null}
								/>
							</Suspense>
						)}
						{activeView === "terminal" && (
							<div className="h-full">
								{normalizedWorkspacePath ? (
									<Suspense fallback={viewLoadingFallback}>
										<TerminalView workspacePath={normalizedWorkspacePath} />
									</Suspense>
								) : (
									<EmptyWorkspacePanel
										label={
											locale === "de"
												? "Kein Arbeitsbereich für Terminal"
												: "No workspace for terminal"
										}
									/>
								)}
							</div>
						)}
						<div
							className={cn(
								"h-full",
								activeView !== "browser" && "hidden",
							)}
						>
							<Suspense fallback={viewLoadingFallback}>
								<BrowserView
									sessionId={selectedChatSessionId}
									workspacePath={normalizedWorkspacePath}
									className="h-full"
									onSendToChat={handleSendToChat}
									onExpand={() => setExpandedView("browser")}
								/>
							</Suspense>
						</div>
						{activeView === "settings" && (
							<Suspense fallback={viewLoadingFallback}>
								<PiSettingsView
									locale={locale}
									sessionId={selectedChatSessionId}
									workspacePath={normalizedWorkspacePath}
								/>
							</Suspense>
						)}
					</div>
				</div>
			)}

			{/* Desktop layout */}
			{!isMobileLayout && (
				<div className="hidden lg:flex flex-1 min-h-0 gap-4 items-start">
					<div className="flex-[3] min-w-0 bg-card border border-border p-4 xl:p-6 flex flex-col min-h-0 h-full relative">
						{expandedView === "canvas" ? (
							<div className="flex-1 min-h-0 flex flex-col -m-4 xl:-m-6">
								<div className="flex items-center justify-between px-3 py-1.5 border-b border-border bg-muted/30">
									<span className="text-sm font-medium text-muted-foreground">Canvas</span>
									<button
										type="button"
										onClick={() => setExpandedView(null)}
										className="p-1 text-muted-foreground hover:text-foreground hover:bg-muted/50 rounded"
										aria-label="Collapse canvas back to sidebar"
									>
										<Minimize2 className="w-4 h-4" />
									</button>
								</div>
								<div className="flex-1 min-h-0">
									<Suspense fallback={viewLoadingFallback}>
										<CanvasView workspacePath={normalizedWorkspacePath} onSaveAndAddToChat={handleCanvasSaveAndAddToChat} />
									</Suspense>
								</div>
							</div>
						) : expandedView === "browser" ? (
							<div className="flex-1 min-h-0 flex flex-col -m-4 xl:-m-6">
								<Suspense fallback={viewLoadingFallback}>
									<BrowserView
										sessionId={selectedChatSessionId}
										workspacePath={normalizedWorkspacePath}
										className="h-full"
										onSendToChat={handleSendToChat}
										onCollapse={() => setExpandedView(null)}
									/>
								</Suspense>
							</div>
						) : (
							<>
								<div className="absolute top-4 right-4 xl:top-6 xl:right-6 flex items-center gap-1 z-10">
									<button
										type="button"
										onClick={handleNewChat}
										className="p-1.5 text-muted-foreground hover:text-foreground hover:bg-muted/50 rounded transition-colors"
										title={locale === "de" ? "Neue Sitzung" : "New chat"}
									>
										<Plus className="w-4 h-4" />
									</button>
									<button
										type="button"
										onClick={() => setIsSearchOpen((prev) => !prev)}
										className="p-1.5 text-muted-foreground hover:text-foreground hover:bg-muted/50 rounded transition-colors"
										title={isSearchOpen ? "Close search (Esc)" : "Search (Ctrl+F)"}
									>
										{isSearchOpen ? (
											<X className="w-4 h-4" />
										) : (
											<Search className="w-4 h-4" />
										)}
									</button>
									<button
										type="button"
										onClick={() => setRightSidebarCollapsed((prev) => !prev)}
										className="p-1.5 text-muted-foreground hover:text-foreground hover:bg-muted/50 rounded transition-colors"
										title={
											rightSidebarCollapsed ? "Expand sidebar" : "Collapse sidebar"
										}
									>
										{rightSidebarCollapsed ? (
											<PanelLeftClose className="w-4 h-4" />
										) : (
											<PanelRightClose className="w-4 h-4" />
										)}
									</button>
								</div>
								{!isOverviewActive && isSearchOpen && (
									<div className="mb-3 pr-16">
										<ChatSearchBar
											sessionId={selectedChatSessionId ?? undefined}
											onResultSelect={({ lineNumber, messageId }) => {
												const target =
													messageId ?? (lineNumber ? `line-${lineNumber}` : null);
												if (target) setScrollToMessageId(target);
											}}
											isOpen={isSearchOpen}
											onToggle={() => setIsSearchOpen(false)}
											locale={locale}
											hideCloseButton
										/>
									</div>
								)}
								{sessionHeader}
								{showEmptyChat ? (
									<div className="flex-1 flex items-center justify-center text-sm text-muted-foreground">
										{locale === "de" ? "Keine Sitzungen" : "No sessions yet"}
									</div>
								) : isOverviewActive ? (
									overviewPanel
								) : (
									chatPanel
								)}
							</>
						)}
					</div>

					<div
						className={cn(
							"bg-card border border-border flex flex-col min-h-0 h-full transition-all duration-200",
							rightSidebarCollapsed
								? "w-12 items-center"
								: "flex-[2] min-w-[320px] max-w-[420px]",
						)}
					>
						{rightSidebarCollapsed ? (
							<div className="flex flex-col gap-1 p-2 h-full overflow-y-auto">
								<CollapsedTabButton
									activeView={activeView}
									onSelect={(view) => {
										setActiveView(view);
										setRightSidebarCollapsed(false);
									}}
									view="tasks"
									icon={ListTodo}
									label="Tasks"
									badge={latestTodos.length}
								/>
								<CollapsedTabButton
									activeView={activeView}
									onSelect={(view) => {
										setActiveView(view);
										setRightSidebarCollapsed(false);
									}}
									view="files"
									icon={FileText}
									label="Files"
								/>
								<CollapsedTabButton
									activeView={activeView}
									onSelect={(view) => {
										setActiveView(view);
										setRightSidebarCollapsed(false);
									}}
									view="canvas"
									icon={PaintBucket}
									label="Canvas"
								/>
								{features.mmry_enabled && (
									<CollapsedTabButton
										activeView={activeView}
										onSelect={(view) => {
											setActiveView(view);
											setRightSidebarCollapsed(false);
										}}
										view="memories"
										icon={Brain}
										label="Memories"
									/>
								)}
								<CollapsedTabButton
									activeView={activeView}
									onSelect={(view) => {
										setActiveView(view);
										setRightSidebarCollapsed(false);
									}}
									view="terminal"
									icon={Terminal}
									label="Terminal"
								/>
								<CollapsedTabButton
									activeView={activeView}
									onSelect={(view) => {
										setActiveView(view);
										setRightSidebarCollapsed(false);
									}}
									view="browser"
									icon={Globe}
									label="Browser"
								/>
								<CollapsedTabButton
									activeView={activeView}
									onSelect={(view) => {
										setActiveView(view);
										setRightSidebarCollapsed(false);
									}}
									view="settings"
									icon={Settings}
									label="Settings"
								/>
							</div>
						) : expandedView === "canvas" ? (
							<div className="flex-1 min-h-0 flex flex-col">
								{sessionHeader}
								{showEmptyChat ? (
									<div className="flex-1 flex items-center justify-center text-sm text-muted-foreground">
										{locale === "de" ? "Keine Sitzungen" : "No sessions yet"}
									</div>
								) : isOverviewActive ? (
									overviewPanel
								) : (
									chatPanel
								)}
							</div>
						) : (
							<>
								<div className="flex gap-1 p-2 border-b border-border">
									<TabButton
										activeView={activeView}
										onSelect={setActiveView}
										view="tasks"
										icon={ListTodo}
										label="Tasks"
										badge={latestTodos.length}
										hideLabel
									/>
									<TabButton
										activeView={activeView}
										onSelect={setActiveView}
										view="files"
										icon={FileText}
										label="Files"
										hideLabel
									/>
									<TabButton
										activeView={activeView}
										onSelect={setActiveView}
										view="canvas"
										icon={PaintBucket}
										label="Canvas"
										hideLabel
									/>
									{features.mmry_enabled && (
										<TabButton
											activeView={activeView}
											onSelect={setActiveView}
											view="memories"
											icon={Brain}
											label="Memories"
											hideLabel
										/>
									)}
									<TabButton
										activeView={activeView}
										onSelect={setActiveView}
										view="terminal"
										icon={Terminal}
										label="Terminal"
										hideLabel
									/>
									<TabButton
										activeView={activeView}
										onSelect={setActiveView}
										view="browser"
										icon={Globe}
										label="Browser"
										hideLabel
									/>
									<TabButton
										activeView={activeView}
										onSelect={setActiveView}
										view="settings"
										icon={Settings}
										label="Settings"
										hideLabel
									/>
								</div>
								<div className="flex-1 min-h-0 overflow-hidden">
									{activeView === "tasks" && tasksPanel}
									{activeView === "files" && (
										<div className="h-full flex flex-col">
											{previewFilePath ? (
												<Suspense fallback={viewLoadingFallback}>
													<PreviewView
														filePath={previewFilePath}
														workspacePath={normalizedWorkspacePath}
														onClose={handleClosePreview}
														showHeader
													/>
												</Suspense>
											) : normalizedWorkspacePath ? (
												<Suspense fallback={viewLoadingFallback}>
													<FileTreeView
														workspacePath={normalizedWorkspacePath}
														onPreviewFile={handlePreviewFile}
														onOpenInCanvas={handleOpenInCanvas}
														state={fileTreeState}
														onStateChange={setFileTreeState}
													/>
												</Suspense>
											) : (
												<EmptyWorkspacePanel
													label={
														locale === "de"
															? "Kein Arbeitsbereich für Dateien"
															: "No workspace for files"
													}
												/>
											)}
										</div>
									)}
									{activeView === "canvas" && (
										<div className="h-full flex flex-col">
											<div className="flex items-center justify-between px-2 py-1 border-b border-border bg-muted/30">
												<span className="text-xs text-muted-foreground">
													Canvas
												</span>
												<button
													type="button"
													onClick={() =>
														setExpandedView(
															expandedView === "canvas" ? null : "canvas",
														)
													}
													className="p-1 text-muted-foreground hover:text-foreground hover:bg-muted/50"
													aria-label={
														expandedView === "canvas"
															? "Collapse canvas"
															: "Expand canvas"
													}
												>
													{expandedView === "canvas" ? (
														<Minimize2 className="w-3.5 h-3.5" />
													) : (
														<Maximize2 className="w-3.5 h-3.5" />
													)}
												</button>
											</div>
											<div className="flex-1 min-h-0">
												<Suspense fallback={viewLoadingFallback}>
													<CanvasView workspacePath={normalizedWorkspacePath} onSaveAndAddToChat={handleCanvasSaveAndAddToChat} />
												</Suspense>
											</div>
										</div>
									)}
									{features.mmry_enabled && activeView === "memories" && (
										<Suspense fallback={viewLoadingFallback}>
											<MemoriesView
												workspacePath={normalizedWorkspacePath}
												storeName={null}
											/>
										</Suspense>
									)}
									{activeView === "terminal" && (
										<div className="h-full">
											{normalizedWorkspacePath ? (
												<Suspense fallback={viewLoadingFallback}>
													<TerminalView
														workspacePath={normalizedWorkspacePath}
													/>
												</Suspense>
											) : (
												<EmptyWorkspacePanel
													label={
														locale === "de"
															? "Kein Arbeitsbereich für Terminal"
															: "No workspace for terminal"
													}
												/>
											)}
										</div>
									)}
									<div
										className={cn(
											"h-full",
											activeView !== "browser" && "hidden",
										)}
									>
										<Suspense fallback={viewLoadingFallback}>
											<BrowserView
												sessionId={selectedChatSessionId}
												workspacePath={normalizedWorkspacePath}
												className="h-full"
												onSendToChat={handleSendToChat}
											/>
										</Suspense>
									</div>
									{activeView === "settings" && (
										<Suspense fallback={viewLoadingFallback}>
											<PiSettingsView
												locale={locale}
												sessionId={selectedChatSessionId}
												workspacePath={normalizedWorkspacePath}
											/>
										</Suspense>
									)}
								</div>
							</>
						)}
					</div>
				</div>
			)}
		</div>
	);
});
