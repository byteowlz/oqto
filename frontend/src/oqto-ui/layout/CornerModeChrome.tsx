import {
	ArrowLeftRight,
	Bot,
	Cpu,
	FileText,
	GitFork,
	Images,
	MessageSquare,
	Mic,
	PanelLeft,
	Paperclip,
	Plus,
	Search,
	Send,
	Terminal,
	X,
} from "lucide-react";
import {
	type KeyboardEvent,
	type PointerEvent,
	type ReactNode,
	useRef,
	useState,
} from "react";
import { useTranslation } from "react-i18next";
import type {
	OqtoUiConfig,
	SessionOverview,
	UiNavigation,
	WorkDirectory,
} from "../platform/contracts";
import {
	type HoldIntentTimer,
	cancelHoldIntent,
	scheduleHoldIntent,
} from "../platform/hold-intent";

type OpenSurface =
	| "navigator"
	| "projects"
	| "tools"
	| "mru"
	| "actions"
	| null;

type CornerModeChromeProps = {
	children: ReactNode;
	config: OqtoUiConfig;
	directories: WorkDirectory[];
	directory: WorkDirectory;
	session: SessionOverview;
	onNavigate: (next: UiNavigation) => void;
};

type CornerButtonProps = {
	label: string;
	children: ReactNode;
	open: boolean;
	holdMs: number;
	onTap: () => void;
	onHold: () => void;
};

function CornerButton({
	label,
	children,
	open,
	holdMs,
	onTap,
	onHold,
}: CornerButtonProps) {
	const timer = useRef<HoldIntentTimer | null>(null);
	const held = useRef(false);
	const clear = () => {
		cancelHoldIntent(timer.current);
		timer.current = null;
	};
	const start = (event: PointerEvent<HTMLButtonElement>) => {
		held.current = false;
		event.currentTarget.setPointerCapture(event.pointerId);
		timer.current = scheduleHoldIntent(holdMs, () => {
			held.current = true;
			onHold();
		});
	};
	const finish = () => {
		clear();
		if (!held.current) onTap();
	};
	const key = (event: KeyboardEvent<HTMLButtonElement>) => {
		if (event.key === "ArrowDown") {
			event.preventDefault();
			onHold();
		}
	};
	return (
		<button
			className="wb-corner-button"
			data-open={open}
			type="button"
			aria-label={label}
			aria-haspopup="menu"
			onPointerDown={start}
			onPointerUp={finish}
			onPointerCancel={clear}
			onKeyDown={key}
		>
			{children}
			<span className="wb-corner-button__hold" aria-hidden="true" />
		</button>
	);
}

function firstSession(directory: WorkDirectory): SessionOverview | null {
	return directory.sessions[0] ?? null;
}

/**
 * Real committed logo when the platform found one; deterministic initials
 * otherwise (house rule: real logos first, procedural fallback second).
 */
type DirectoryFaceProps = {
	directory: WorkDirectory;
};

function DirectoryFace({ directory }: DirectoryFaceProps) {
	if (directory.logoUrl) {
		return (
			<img
				className="wb-corner-face"
				src={directory.logoUrl}
				alt=""
				aria-hidden="true"
			/>
		);
	}
	return <span>{directory.accent}</span>;
}

function allSessions(directories: WorkDirectory[]): Array<{
	directory: WorkDirectory;
	session: SessionOverview;
}> {
	return directories.flatMap((directory) =>
		directory.sessions.map((session) => ({ directory, session })),
	);
}

export function CornerModeChrome({
	children,
	config,
	directories,
	directory,
	session,
	onNavigate,
}: CornerModeChromeProps) {
	const { t } = useTranslation();
	const [open, setOpen] = useState<OpenSurface>(null);
	const [query, setQuery] = useState("");
	const [seen, setSeen] = useState<ReadonlySet<string>>(new Set());
	const [mru, setMru] = useState<
		Array<{ directory: WorkDirectory; session: SessionOverview }>
	>([]);
	const sessions = allSessions(directories);
	const currentIndex = sessions.findIndex(
		(item) => item.session.id === session.id,
	);
	const fallbackPrevious =
		sessions.at(currentIndex - 1) ?? sessions.at(-1) ?? null;
	const previous = mru[0] ?? fallbackPrevious;
	const candidates = (mru.length > 0 ? mru : sessions)
		.filter((item) => item.session.id !== session.id)
		.slice(0, 5);
	const filtered = sessions.filter(
		({ directory: itemDirectory, session: item }) =>
			`${item.name} ${item.id} ${itemDirectory.name}`
				.toLowerCase()
				.includes(query.toLowerCase()),
	);
	const toggle = (surface: Exclude<OpenSurface, null>) => {
		setOpen((current) => (current === surface ? null : surface));
	};
	const statusFor = (item: SessionOverview) =>
		item.status === "done" && seen.has(item.id) ? "idle" : item.status;
	const directoryAttention = (itemDirectory: WorkDirectory) => {
		const statuses = itemDirectory.sessions.map(statusFor);
		if (statuses.includes("blocked")) return "blocked";
		if (statuses.includes("done")) return "done";
		if (statuses.includes("working")) return "working";
		return "idle";
	};
	const navigateSession = (
		itemDirectory: WorkDirectory,
		item: SessionOverview,
	) => {
		setMru((current) =>
			[
				{ directory, session },
				...current.filter((entry) => entry.session.id !== session.id),
			].slice(0, 8),
		);
		setSeen((current) => new Set(current).add(item.id));
		onNavigate({
			workDirectoryId: itemDirectory.id,
			sessionId: item.id,
			mobileView: "chat",
		});
		setOpen(null);
	};
	const openMenu = (menuId: string) => {
		const surface: OpenSurface = {
			"menu.projects": "projects",
			"menu.tools": "tools",
			"menu.sessionMru": "mru",
			"menu.chatActions": "actions",
		}[menuId] as OpenSurface;
		if (surface) setOpen(surface);
	};
	const runAction = (actionId: string) => {
		if (actionId === "navigator.open") toggle("navigator");
		if (actionId === "view.openFiles") onNavigate({ mobileView: "files" });
		if (actionId === "view.openChat") onNavigate({ mobileView: "chat" });
		if (actionId === "session.openPrevious" && previous) {
			navigateSession(previous.directory, previous.session);
		}
	};

	return (
		<div className="wb-corner-frame" data-mode={config.mobile.mode}>
			<header className="wb-corner-top">
				<CornerButton
					label={t("oqtoUi.corner.navigator")}
					open={open === "navigator" || open === "projects"}
					holdMs={config.mobile.hold_ms}
					onTap={() => runAction(config.mobile.corners.top_left.tap)}
					onHold={() => openMenu(config.mobile.corners.top_left.hold)}
				>
					<PanelLeft aria-hidden="true" />
				</CornerButton>
				<div className="wb-corner-identity">
					<strong>{session.name}</strong>
					<small>
						{directory.name} [{session.id}]
					</small>
				</div>
				<CornerButton
					label={t("oqtoUi.corner.tools")}
					open={open === "tools"}
					holdMs={config.mobile.hold_ms}
					onTap={() => runAction(config.mobile.corners.top_right.tap)}
					onHold={() => openMenu(config.mobile.corners.top_right.hold)}
				>
					<FileText aria-hidden="true" />
				</CornerButton>
			</header>

			<div className="wb-corner-content">{children}</div>

			<div className="wb-corner-composer">
				<CornerButton
					label={t("oqtoUi.corner.previousSession")}
					open={open === "mru"}
					holdMs={config.mobile.hold_ms}
					onTap={() => runAction(config.mobile.corners.bottom_left.tap)}
					onHold={() => openMenu(config.mobile.corners.bottom_left.hold)}
				>
					<ArrowLeftRight aria-hidden="true" />
				</CornerButton>
				<textarea
					rows={1}
					placeholder={t("oqtoUi.chat.placeholder")}
					aria-label={t("oqtoUi.chat.placeholder")}
				/>
				<CornerButton
					label={t("oqtoUi.corner.sendActions")}
					open={open === "actions"}
					holdMs={config.mobile.hold_ms}
					onTap={() => runAction(config.mobile.corners.bottom_right.tap)}
					onHold={() => openMenu(config.mobile.corners.bottom_right.hold)}
				>
					<Send aria-hidden="true" />
				</CornerButton>
			</div>

			<div className="wb-corner-status">
				{config.status_line.segments.includes("session") ? (
					<span data-status={statusFor(session)}>
						<i /> {statusFor(session)}
					</span>
				) : null}
				{config.status_line.segments.includes("model") ? (
					<span>{session.model}</span>
				) : null}
				{config.status_line.segments.includes("context") && session.context ? (
					<span>
						{session.context.tokens} · {session.context.percent}%
					</span>
				) : null}
				{config.status_line.segments.includes("connection") ? (
					<span>{t("oqtoUi.customization.file")}</span>
				) : null}
			</div>

			{open ? (
				<button
					className="wb-corner-scrim"
					type="button"
					aria-label={t("oqtoUi.corner.close")}
					onClick={() => setOpen(null)}
				/>
			) : null}

			{open === "navigator" ? (
				<section
					className="wb-corner-navigator"
					aria-label={t("oqtoUi.corner.navigator")}
				>
					<nav
						className="wb-corner-ribbon"
						aria-label={t("oqtoUi.corner.projects")}
					>
						<button type="button" aria-label={t("oqtoUi.corner.newProject")}>
							<Plus aria-hidden="true" />
						</button>
						{directories.map((item) => (
							<button
								key={item.id}
								type="button"
								data-active={item.id === directory.id}
								title={item.name}
								onClick={() => {
									const next = firstSession(item);
									if (next) navigateSession(item, next);
								}}
							>
								<DirectoryFace directory={item} />
								<small>{item.name.slice(0, 7)}</small>
								<i
									className="wb-corner-attention"
									data-status={directoryAttention(item)}
								/>
							</button>
						))}
					</nav>
					<div className="wb-corner-nav-main">
						<header>
							<Bot aria-hidden="true" />
							<span>
								<strong>
									{t("oqtoUi.corner.projectsAndSessions", {
										count: directories.length,
									})}
								</strong>
								<small>{directory.path}</small>
							</span>
							<button type="button" onClick={() => setOpen("projects")}>
								{t("oqtoUi.corner.switch")}
							</button>
						</header>
						<label className="wb-corner-search">
							<Search aria-hidden="true" />
							<input
								value={query}
								onChange={(event) => setQuery(event.target.value)}
								placeholder={t("oqtoUi.corner.search")}
							/>
						</label>
						<div className="wb-corner-session-list">
							<button className="wb-corner-new" type="button">
								<Plus aria-hidden="true" /> {t("oqtoUi.corner.newSession")}
							</button>
							{filtered.map((item) => (
								<button
									key={item.session.id}
									type="button"
									data-current={item.session.id === session.id}
									onClick={() => navigateSession(item.directory, item.session)}
								>
									<i data-status={statusFor(item.session)} />
									<span>
										<strong>
											{item.session.name} <em>[{item.session.id}]</em>
										</strong>
										<small>
											{item.session.updated} · {item.directory.name}
										</small>
									</span>
								</button>
							))}
						</div>
						<footer>
							{t("oqtoUi.corner.navigatorStatus", {
								name: directory.name,
								count: directory.sessions.length,
							})}
						</footer>
					</div>
				</section>
			) : null}

			{open === "projects" ? (
				<MenuSheet
					title={t("oqtoUi.corner.projects")}
					onClose={() => setOpen(null)}
				>
					{directories.map((item) => (
						<button
							key={item.id}
							type="button"
							data-current={item.id === directory.id}
							onClick={() => {
								const next = firstSession(item);
								if (next) navigateSession(item, next);
							}}
						>
							<DirectoryFace directory={item} />
							<strong>{item.name}</strong>
							<small>
								{t("oqtoUi.corner.sessionCount", {
									count: item.sessions.length,
								})}
							</small>
						</button>
					))}
				</MenuSheet>
			) : null}

			{open === "tools" ? (
				<MenuSheet
					title={t("oqtoUi.corner.tools")}
					onClose={() => setOpen(null)}
				>
					<MenuAction
						icon={<FileText />}
						label={t("oqtoUi.mobile.files")}
						onSelect={() => {
							onNavigate({ mobileView: "files" });
							setOpen(null);
						}}
					/>
					<MenuAction
						icon={<MessageSquare />}
						label={t("oqtoUi.mobile.chat")}
						onSelect={() => {
							onNavigate({ mobileView: "chat", workAreaTab: "chat" });
							setOpen(null);
						}}
					/>
					<MenuAction
						icon={<Terminal />}
						label={t("oqtoUi.tools.terminal")}
						onSelect={() => {
							onNavigate({ mobileView: "chat", workAreaTab: "terminal" });
							setOpen(null);
						}}
					/>
					<MenuAction
						icon={<Images />}
						label={t("oqtoUi.tools.gallery")}
						onSelect={() => {
							onNavigate({ mobileView: "chat", workAreaTab: "gallery" });
							setOpen(null);
						}}
					/>
				</MenuSheet>
			) : null}

			{open === "mru" ? (
				<MenuSheet
					title={t("oqtoUi.corner.recentSessions")}
					onClose={() => setOpen(null)}
				>
					{candidates.map((item) => (
						<button
							key={item.session.id}
							type="button"
							onClick={() => navigateSession(item.directory, item.session)}
						>
							<i data-status={statusFor(item.session)} />
							<strong>{item.session.name}</strong>
							<small>{item.directory.name}</small>
						</button>
					))}
				</MenuSheet>
			) : null}

			{open === "actions" ? (
				<div
					className="wb-corner-radial"
					role="menu"
					aria-label={t("oqtoUi.corner.sendActions")}
				>
					<button
						type="button"
						role="menuitem"
						data-action="attach"
						aria-label={t("oqtoUi.chat.attach")}
					>
						<Paperclip aria-hidden="true" />
					</button>
					<button
						type="button"
						role="menuitem"
						data-action="voice"
						aria-label={t("oqtoUi.corner.voice")}
					>
						<Mic aria-hidden="true" />
					</button>
					<button
						type="button"
						role="menuitem"
						data-action="model"
						aria-label={t("oqtoUi.corner.model")}
					>
						<Cpu aria-hidden="true" />
					</button>
					<button
						type="button"
						role="menuitem"
						data-action="fork"
						aria-label={t("oqtoUi.chat.forkHere")}
					>
						<GitFork aria-hidden="true" />
					</button>
					<button
						className="wb-corner-radial__cancel"
						type="button"
						aria-label={t("oqtoUi.corner.close")}
						onClick={() => setOpen(null)}
					>
						<X aria-hidden="true" />
					</button>
				</div>
			) : null}
		</div>
	);
}

type MenuSheetProps = {
	title: string;
	children: ReactNode;
	onClose: () => void;
};
function MenuSheet({ title, children, onClose }: MenuSheetProps) {
	return (
		<section className="wb-corner-menu" role="menu" aria-label={title}>
			<header>
				<strong>{title}</strong>
				<button type="button" onClick={onClose}>
					<X aria-hidden="true" />
				</button>
			</header>
			<div>{children}</div>
		</section>
	);
}

type MenuActionProps = { icon: ReactNode; label: string; onSelect: () => void };
function MenuAction({ icon, label, onSelect }: MenuActionProps) {
	return (
		<button type="button" role="menuitem" onClick={onSelect}>
			{icon}
			<strong>{label}</strong>
		</button>
	);
}
