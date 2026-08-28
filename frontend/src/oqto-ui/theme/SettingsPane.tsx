import { X } from "lucide-react";
import { useTranslation } from "react-i18next";
import type {
	OqtoUiConfigResolution,
	UiNavigation,
} from "../platform/contracts";
import { ConfigLens } from "./ConfigLens";
import { ThemeCustomizer } from "./ThemeCustomizer";
import { ThemePicker } from "./ThemePicker";
import type { OqtoUiUserTheme } from "./userTheme";

type SettingsPaneProps = {
	schemeId: string;
	themeRoot: HTMLElement | null;
	userTheme: OqtoUiUserTheme;
	resolution: OqtoUiConfigResolution;
	onChange: (next: OqtoUiUserTheme) => void;
	onNavigate: (next: UiNavigation) => void;
	onClose: () => void;
};

/** Temporary settings View occupying the Files View slot. */
export function SettingsPane({
	schemeId,
	themeRoot,
	userTheme,
	resolution,
	onChange,
	onNavigate,
	onClose,
}: SettingsPaneProps) {
	const { t } = useTranslation();
	return (
		<aside
			className="wb-panel wb-settings-pane"
			aria-label={t("oqtoUi.settings.label")}
		>
			<header className="wb-settings-pane__header">
				<strong>{t("oqtoUi.settings.label")}</strong>
				<button
					className="wb-icon-button"
					type="button"
					aria-label={t("oqtoUi.settings.close")}
					onClick={onClose}
				>
					<X aria-hidden="true" />
				</button>
			</header>
			<section className="wb-settings-pane__section">
				<h2>{t("oqtoUi.theme.label")}</h2>
				<ThemePicker schemeId={schemeId} onNavigate={onNavigate} />
			</section>
			<ThemeCustomizer
				embedded
				themeRoot={themeRoot}
				userTheme={userTheme}
				onChange={onChange}
			/>
			<ConfigLens resolution={resolution} />
		</aside>
	);
}
