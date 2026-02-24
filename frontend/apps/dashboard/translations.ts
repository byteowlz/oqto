// Dashboard translations have been moved to messages/en.json and messages/de.json
// under the "dashboard" section. Use useTranslation() from react-i18next instead.
//
// This file is kept for backward compatibility but re-exports from i18n.
import { i18n } from "@/lib/i18n";

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

export function getTranslations(_locale: "de" | "en"): DashboardTranslations {
	return {
		title: i18n.t("dashboard.title"),
		subtitle: i18n.t("dashboard.subtitle"),
		stats: i18n.t("dashboard.stats"),
		scheduler: i18n.t("dashboard.scheduler"),
		workingAgents: i18n.t("dashboard.workingAgents"),
		trx: i18n.t("dashboard.trx"),
		feeds: i18n.t("dashboard.feeds"),
		addFeed: i18n.t("dashboard.addFeed"),
		reload: i18n.t("dashboard.reload"),
		noTasks: i18n.t("dashboard.noTasks"),
		noAgents: i18n.t("dashboard.noAgents"),
		noTrx: i18n.t("dashboard.noTrx"),
		noFeeds: i18n.t("dashboard.noFeeds"),
		layout: i18n.t("dashboard.layout"),
		customCards: i18n.t("dashboard.customCards"),
		notice: i18n.t("dashboard.notice"),
	};
}
