import { useTranslation } from "react-i18next";
import type { OqtoUiConfigResolution } from "../platform/contracts";

type ConfigLensProps = {
	resolution: OqtoUiConfigResolution;
};

export function ConfigLens({ resolution }: ConfigLensProps) {
	const { t } = useTranslation();
	const diagnostic = resolution.diagnostics[0];
	return (
		<div className="wb-config-lens" data-invalid={diagnostic !== undefined}>
			<span>{t("oqtoUi.customization.file")}</span>
			<strong>{resolution.config.preset}</strong>
			<span>
				{t("oqtoUi.customization.bindings", {
					count: resolution.config.bindings.length,
				})}
			</span>
			{diagnostic ? (
				<span title={diagnostic.message}>{diagnostic.code}</span>
			) : null}
		</div>
	);
}
