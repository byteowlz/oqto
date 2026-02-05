"use client";

import { ContextWindowGauge } from "@/components/data-display";
import { ChatSearchBar, ChatView, PiSettingsView } from "@/features/chat";
import {
	type Features,
	getFeatures,
	newWorkspacePiSession,
} from "@/features/chat/api";
import { useApp } from "@/hooks/use-app";
import { useIsMobile } from "@/hooks/use-mobile";
import { isPendingSessionId, normalizeWorkspacePath } from "@/lib/session-utils";
import { cn } from "@/lib/utils";
import {
	Brain,
	CircleDot,
	FileText,
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
	Terminal,
	X,
} from "lucide-react";
import {
	Suspense,
	lazy,
	memo,
	type ComponentType,
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
const PreviewView = lazy(() =>
	import("@/features/sessions/components/PreviewView").then((mod) => ({
		default: mod.PreviewView,
	})),
);

type ViewKey =
	| "chat"
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
	return (
		<div
			className={cn(
				"flex flex-col gap-2 overflow-auto p-2",
				fullHeight && "h-full",
			)}
		>
			{todos.map((todo) => (
				<div
					key={todo.id}
					className="flex items-start gap-2 px-3 py-2 border border-border rounded"
				>
					<div className="flex-1">
						<div className="text-sm text-foreground">{todo.content}</div>
						<div className="text-xs text-muted-foreground">
							{todo.status.replace("_", " ")} • {todo.priority}
						</div>
					</div>
				</div>
			))}
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

	const handlePreviewFile = useCallback((filePath: string) => {
		setPreviewFilePath(filePath);
	}, []);

	const handleClosePreview = useCallback(() => {
		setPreviewFilePath(null);
	}, []);

	const normalizedWorkspacePath = useMemo(
		() => normalizeWorkspacePath(selectedChatFromHistory?.workspace_path),
		[selectedChatFromHistory?.workspace_path],
	);

	const handleEnsureSession = useCallback(
		async (workspacePath: string | null, optimisticId: string | null) => {
			try {
				const resolvedPath = normalizeWorkspacePath(workspacePath);
				if (!resolvedPath) {
					return null;
				}
				const state = await newWorkspacePiSession(resolvedPath);
				if (state.session_id) {
					if (
						optimisticId &&
						optimisticId !== state.session_id &&
						isPendingSessionId(optimisticId)
					) {
						replaceOptimisticChatSession(optimisticId, state.session_id);
					}
					setSelectedChatSessionId(state.session_id);
					refreshChatHistory();
					return state.session_id;
				}
				return null;
			} catch (err) {
				if (optimisticId) {
					clearOptimisticChatSession(optimisticId);
				}
				console.error("Failed to ensure Pi session:", err);
				return null;
			}
		},
		[
			clearOptimisticChatSession,
			refreshChatHistory,
			replaceOptimisticChatSession,
			setSelectedChatSessionId,
		],
	);

	const handleNewChat = useCallback(async () => {
		const id = await createNewChat(normalizedWorkspacePath ?? undefined);
		if (id) setSelectedChatSessionId(id);
	}, [
		createNewChat,
		normalizedWorkspacePath,
		setSelectedChatSessionId,
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

	// Clear file preview when session changes
	// biome-ignore lint/correctness/useExhaustiveDependencies: intentionally reset on session change
	useEffect(() => {
		setPreviewFilePath(null);
	}, [selectedChatSessionId]);

	const headerTitle =
		selectedChatFromHistory?.title ??
		(locale === "de" ? "Chat" : "Chat");
	const readableId = selectedChatFromHistory?.readable_id ?? null;
	const workspaceName =
		normalizedWorkspacePath?.split("/").filter(Boolean).pop() ?? null;
	const formattedDate = selectedChatFromHistory?.created_at
		? new Date(selectedChatFromHistory.created_at).toLocaleDateString(
				locale === "de" ? "de-DE" : "en-US",
			)
		: null;

	const chatPanel = normalizedWorkspacePath ? (
		<ChatView
			locale={locale}
			className="flex-1"
			features={features}
			workspacePath={normalizedWorkspacePath}
			selectedSessionId={selectedChatSessionId}
			onSelectedSessionIdChange={setSelectedChatSessionId}
			onEnsureSession={handleEnsureSession}
			scrollToMessageId={scrollToMessageId}
			onScrollToMessageComplete={() => setScrollToMessageId(null)}
			onTokenUsageChange={setTokenUsage}
			onTodosChange={setLatestTodos}
			onMessageSent={refreshChatHistory}
			onMessageComplete={refreshChatHistory}
			hideHeader
		/>
	) : (
		<EmptyWorkspacePanel
			label={locale === "de" ? "Lade Chat..." : "Loading chat..."}
		/>
	);

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

	const sessionHeader = (
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
							{readableId && ` [${readableId}]`}
						</span>
					)}
					{workspaceName && readableId && formattedDate && (
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

	const showEmptyChat =
		!selectedChatSessionId && chatHistory.length === 0 && featuresLoaded;

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
						<div className={cn("h-full flex flex-col", activeView !== "files" && "hidden")}>
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
									<CanvasView workspacePath={normalizedWorkspacePath} />
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
						{activeView === "browser" && (
							<div className="h-full">
								<Suspense fallback={viewLoadingFallback}>
									<BrowserView
										sessionId={selectedChatSessionId ?? "browser"}
										className="h-full"
									/>
								</Suspense>
							</div>
						)}
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
						{isSearchOpen && (
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
								{locale === "de"
									? "Keine Sitzungen"
									: "No sessions yet"}
							</div>
						) : (
							chatPanel
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
													<CanvasView
														workspacePath={normalizedWorkspacePath}
													/>
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
									{activeView === "browser" && (
										<div className="h-full">
											<Suspense fallback={viewLoadingFallback}>
												<BrowserView
													sessionId={selectedChatSessionId ?? "browser"}
													className="h-full"
												/>
											</Suspense>
										</div>
									)}
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
