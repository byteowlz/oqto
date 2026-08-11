import { useTranslation } from "react-i18next";
import type { LabNavigation } from "../../modules/lab/model";

type ThemePickerProps = {
	schemeId: string;
	onNavigate: (next: LabNavigation) => void;
};

const schemeOptions = [
	"oqto-dark",
	"oqto-light",
	"nord-dark",
	"nord-light",
] as const;

export function ThemePicker({ schemeId, onNavigate }: ThemePickerProps) {
	const { t } = useTranslation();
	return (
		<div
			className="wb-theme-picker"
			aria-label={t("workbench.theme.label")}
			role="toolbar"
		>
			{schemeOptions.map((option) => (
				<button
					aria-pressed={schemeId === option}
					className="wb-theme-picker__option"
					data-active={schemeId === option}
					key={option}
					type="button"
					onClick={() => onNavigate({ schemeId: option })}
				>
					{t(`workbench.theme.${option}`)}
				</button>
			))}
		</div>
	);
}
