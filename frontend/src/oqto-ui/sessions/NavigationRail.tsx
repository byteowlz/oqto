import {
	ArrowDown,
	ChevronDown,
	ChevronRight,
	Folder,
	FolderPlus,
	LogOut,
	MessageSquare,
	PanelLeftClose,
	Plus,
	Search,
	Settings,
	Shield,
	Sun,
} from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import type { UiNavigation, WorkDirectory } from "../platform/contracts";

type NavigationRailProps = {
	workDirectories: WorkDirectory[];
	workDirectoryId: string;
	sessionId: string;
	schemeId: string;
	onNavigate: (next: UiNavigation) => void;
};

export function NavigationRail({
	workDirectories,
	workDirectoryId,
	sessionId,
	schemeId,
	onNavigate,
}: NavigationRailProps) {
	const { t, i18n } = useTranslation();
	const [disclosedIds, setDisclosedIds] = useState<ReadonlySet<string>>(
		() => new Set([workDirectoryId]),
	);
	const toggleDisclosure = (id: string) => {
		setDisclosedIds((current) => {
			const next = new Set(current);
			if (next.has(id)) {
				next.delete(id);
			} else {
				next.add(id);
			}
			return next;
		});
	};
	const discloseAnd = (id: string, next: UiNavigation) => {
		setDisclosedIds((current) => new Set(current).add(id));
		onNavigate(next);
	};
	const sessionCount = workDirectories.reduce(
		(sum, directory) => sum + directory.sessions.length,
		0,
	);
	const logoSrc = schemeId.endsWith("light")
		? "/oqto_logo_black.svg"
		: "/oqto_logo_white.svg";
	return (
		<aside className="wb-sidebar" aria-label={t("oqtoUi.navigation.label")}>
			<header className="wb-sidebar__logo">
				<img src={logoSrc} alt={t("oqtoUi.brandLabel")} />
				<button
					className="wb-icon-button"
					type="button"
					aria-label={t("oqtoUi.navigation.collapse")}
				>
					<PanelLeftClose aria-hidden="true" />
				</button>
			</header>
			<div className="wb-sidebar__rule" aria-hidden="true" />

			<div className="wb-sidebar__search">
				<button
					className="wb-search-mode"
					type="button"
					aria-label={t("oqtoUi.navigation.searchMode")}
				>
					<Search aria-hidden="true" />
					<ChevronDown aria-hidden="true" />
				</button>
				<input
					placeholder={t("oqtoUi.navigation.searchPlaceholder")}
					type="text"
				/>
			</div>

			<div className="wb-sessions-header">
				<span className="wb-sessions-header__title">
					{t("oqtoUi.navigation.sessions")}
				</span>
				<span className="wb-sessions-header__count">({sessionCount})</span>
				<span className="wb-sessions-header__actions">
					<button
						className="wb-icon-button"
						type="button"
						aria-label={t("oqtoUi.navigation.newSession")}
					>
						<Plus aria-hidden="true" />
					</button>
					<button
						className="wb-icon-button"
						type="button"
						aria-label={t("oqtoUi.navigation.newProject")}
					>
						<FolderPlus aria-hidden="true" />
					</button>
					<button
						className="wb-icon-button"
						type="button"
						aria-label={t("oqtoUi.navigation.sortSessions")}
					>
						<ArrowDown aria-hidden="true" />
					</button>
				</span>
			</div>

			<nav
				className="wb-session-tree"
				aria-label={t("oqtoUi.navigation.workDirectories")}
			>
				{workDirectories.map((directory) => {
					const selected = directory.id === workDirectoryId;
					const disclosed = disclosedIds.has(directory.id);
					return (
						<section className="wb-project" key={directory.id}>
							<div className="wb-project__row" data-active={selected}>
								<button
									className="wb-project__disclosure"
									type="button"
									aria-expanded={disclosed}
									aria-label={t("oqtoUi.navigation.toggleSessions", {
										name: directory.name,
									})}
									onClick={() => toggleDisclosure(directory.id)}
								>
									{disclosed ? (
										<ChevronDown aria-hidden="true" />
									) : (
										<ChevronRight aria-hidden="true" />
									)}
								</button>
								<button
									className="wb-project__select"
									type="button"
									onClick={() =>
										discloseAnd(directory.id, {
											workDirectoryId: directory.id,
											sessionId: directory.sessions[0]?.id,
										})
									}
								>
									<Folder aria-hidden="true" />
									<span className="wb-project__name">{directory.name}</span>
									<span className="wb-project__count">
										({directory.sessions.length})
									</span>
								</button>
							</div>
							{disclosed ? (
								<div className="wb-project__sessions">
									{directory.sessions.map((session) => (
										<button
											className="wb-session-item"
											data-active={session.id === sessionId}
											key={session.id}
											type="button"
											onClick={() =>
												onNavigate({
													workDirectoryId: directory.id,
													sessionId: session.id,
												})
											}
										>
											<MessageSquare aria-hidden="true" />
											<span className="wb-session-item__copy">
												<span className="wb-session-item__title">
													{session.name}
												</span>
												<span className="wb-session-item__date">
													{session.updated}
												</span>
											</span>
										</button>
									))}
								</div>
							) : null}
						</section>
					);
				})}
			</nav>

			<footer className="wb-sidebar__footer">
				<div className="wb-sidebar__preview">
					<button
						className="wb-language-button"
						type="button"
						onClick={() =>
							void i18n.changeLanguage(i18n.language === "de" ? "en" : "de")
						}
					>
						{i18n.language === "de" ? "EN" : "DE"}
					</button>
				</div>
				<div className="wb-sidebar__rule wb-sidebar__rule--muted" />
				<div className="wb-sidebar__user">{t("oqtoUi.person.name")}</div>
				<div className="wb-sidebar__footer-icons">
					<button
						className="wb-icon-button"
						type="button"
						aria-label={t("oqtoUi.navigation.settings")}
					>
						<Settings aria-hidden="true" />
					</button>
					<button
						className="wb-icon-button"
						type="button"
						aria-label={t("oqtoUi.navigation.admin")}
					>
						<Shield aria-hidden="true" />
					</button>
					<button
						className="wb-icon-button"
						type="button"
						aria-label={t("oqtoUi.navigation.toggleTheme")}
					>
						<Sun aria-hidden="true" />
					</button>
					<button
						className="wb-icon-button"
						type="button"
						aria-label={t("oqtoUi.navigation.logout")}
					>
						<LogOut aria-hidden="true" />
					</button>
				</div>
			</footer>
		</aside>
	);
}
