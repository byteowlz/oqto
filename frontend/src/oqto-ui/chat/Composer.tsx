import { useTranslation } from "react-i18next";
import { OqtoIcon } from "../platform/icons";

export function Composer() {
	const { t } = useTranslation();
	return (
		<footer className="wb-composer">
			<button
				className="wb-icon-button"
				type="button"
				aria-label={t("oqtoUi.chat.attach")}
			>
				<OqtoIcon name="attach" aria-hidden="true" />
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
				<OqtoIcon name="send" aria-hidden="true" />
			</button>
		</footer>
	);
}
