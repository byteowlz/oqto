import { useTranslation } from "react-i18next";
import type { TimelineEntry } from "../platform/contracts";

type ChatViewProps = {
	entries: TimelineEntry[];
};

export function ChatView({ entries }: ChatViewProps) {
	const { t } = useTranslation();
	return (
		<div className="oui-chat">
			<div className="oui-timeline" aria-live="polite">
				{entries.length === 0 ? (
					<p className="oui-empty">{t("oqtoUi.chat.empty")}</p>
				) : (
					entries.map((entry) => (
						<article
							className="oui-entry"
							data-role={entry.role}
							key={entry.id}
						>
							<header>
								<span>{t(`oqtoUi.chat.roles.${entry.role}`)}</span>
								<small>{t(`oqtoUi.chat.status.${entry.status}`)}</small>
							</header>
							<p>{entry.text}</p>
						</article>
					))
				)}
			</div>
			<form className="oui-composer">
				<label htmlFor="oqto-ui-prompt">{t("oqtoUi.chat.composerLabel")}</label>
				<textarea
					id="oqto-ui-prompt"
					name="prompt"
					placeholder={t("oqtoUi.chat.placeholder")}
					rows={3}
				/>
				<div>
					<span>{t("oqtoUi.chat.readOnly")}</span>
					<button type="submit" disabled>
						{t("oqtoUi.chat.send")}
					</button>
				</div>
			</form>
		</div>
	);
}
