import { Paperclip, Send } from "lucide-react";
import { useTranslation } from "react-i18next";

export function Composer() {
	const { t } = useTranslation();
	return (
		<footer className="wb-composer">
			<button
				className="wb-icon-button"
				type="button"
				aria-label={t("oqtoUi.chat.attach")}
			>
				<Paperclip aria-hidden="true" />
			</button>
			<textarea
				rows={1}
				placeholder={t("oqtoUi.chat.placeholder")}
				aria-label={t("oqtoUi.chat.placeholder")}
			/>
			<button
				className="wb-icon-button"
				type="button"
				aria-label={t("oqtoUi.chat.send")}
			>
				<Send aria-hidden="true" />
			</button>
		</footer>
	);
}
