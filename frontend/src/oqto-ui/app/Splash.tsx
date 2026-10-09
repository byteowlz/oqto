import { useTranslation } from "react-i18next";
import "./splash.css";

type SplashProps = {
	schemeId: string;
	error?: boolean;
	onRetry?: () => void;
};

export function Splash({ schemeId, error = false, onRetry }: SplashProps) {
	const { t } = useTranslation();
	const logoSrc = schemeId.endsWith("light")
		? "/oqto_logo_black.svg"
		: "/oqto_logo_white.svg";
	return (
		<div className="wb-splash" aria-live="polite">
			<img
				className="wb-splash__logo"
				data-error={error}
				src={logoSrc}
				alt={t("oqtoUi.brandLabel")}
			/>
			{error ? (
				<>
					<p role="alert">{t("oqtoUi.loadFailed")}</p>
					<button type="button" onClick={onRetry}>
						{t("oqtoUi.retry")}
					</button>
				</>
			) : null}
		</div>
	);
}
