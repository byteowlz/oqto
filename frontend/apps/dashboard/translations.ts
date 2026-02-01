export type DashboardTranslations = {
	title: string;
	subtitle: string;
	stats: string;
	scheduler: string;
	workingAgents: string;
	trx: string;
	feeds: string;
	addFeed: string;
	reload: string;
	noTasks: string;
	noAgents: string;
	noTrx: string;
	noFeeds: string;
	layout: string;
	customCards: string;
	notice: string;
};

const translations: Record<"de" | "en", DashboardTranslations> = {
	de: {
		title: "Dashboard",
		subtitle: "Arbeitsstatus, Scheduler, TRX und Feeds auf einen Blick.",
		stats: "Statusuebersicht",
		scheduler: "Geplante Tasks",
		workingAgents: "Aktive Agents",
		trx: "TRX Ueberblick",
		feeds: "Feeds",
		addFeed: "Feed hinzufuegen",
		reload: "Aktualisieren",
		noTasks: "Keine Schedules gefunden.",
		noAgents: "Keine aktiven Agents.",
		noTrx: "Keine TRX-Issues.",
		noFeeds: "Noch keine Feeds.",
		layout: "Layout",
		customCards: "Custom Cards",
		notice: "Config in .octo/ for the Default Chat workspace.",
	},
	en: {
		title: "Dashboard",
		subtitle: "Workspace status, scheduler, TRX, and feeds at a glance.",
		stats: "Status Overview",
		scheduler: "Scheduled Tasks",
		workingAgents: "Working Agents",
		trx: "TRX Overview",
		feeds: "Feeds",
		addFeed: "Add feed",
		reload: "Refresh",
		noTasks: "No schedules found.",
		noAgents: "No active agents.",
		noTrx: "No TRX issues yet.",
		noFeeds: "No feeds added yet.",
		layout: "Layout",
		customCards: "Custom Cards",
		notice: "Config lives in .octo/ for the Default Chat workspace.",
	},
};

export function getTranslations(locale: "de" | "en"): DashboardTranslations {
	return translations[locale];
}
