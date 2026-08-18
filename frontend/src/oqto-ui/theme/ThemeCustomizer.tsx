import { Palette, RotateCcw, X } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import {
	FONT_MONO_PRESETS,
	FONT_SANS_PRESETS,
	NEUTRAL_TOKEN_HEX,
	type OqtoUiUserTheme,
	SHADOW_PRESET_IDS,
	type ShadowPresetId,
	matchFontPreset,
	matchShadowPreset,
	parseUserThemeJson,
	snapshotTokenHex,
	withShadowPreset,
} from "./userTheme";

type ThemeCustomizerProps = {
	userTheme: OqtoUiUserTheme;
	themeRoot: HTMLElement | null;
	onChange: (next: OqtoUiUserTheme) => void;
};

const COLOR_TOKENS = [
	"--primary",
	"--background",
	"--card",
	"--border",
	"--sidebar",
] as const;

function tokenKey(token: string): string {
	return token.replace(/^--/, "");
}

export function ThemeCustomizer({
	userTheme,
	themeRoot,
	onChange,
}: ThemeCustomizerProps) {
	const { t } = useTranslation();
	const [open, setOpen] = useState(false);
	const [baseHex, setBaseHex] = useState<Record<string, string>>({});
	const [jsonDraft, setJsonDraft] = useState<string | null>(null);
	const [jsonError, setJsonError] = useState<string | null>(null);

	const overrides = userTheme.overrides ?? {};
	const serialized = JSON.stringify(userTheme, null, 2);
	const radiusValue = Number.parseInt(userTheme.radius ?? "0", 10) || 0;
	const blurValue =
		Number.parseInt(overrides["--backdrop-blur"] ?? "0", 10) || 0;
	const shadowPreset = matchShadowPreset(overrides);
	const fontSansPreset = matchFontPreset(FONT_SANS_PRESETS, userTheme.fontSans);
	const fontMonoPreset = matchFontPreset(FONT_MONO_PRESETS, userTheme.fontMono);

	const setFont = (key: "fontSans" | "fontMono", stack: string | undefined) => {
		const { [key]: _removed, ...rest } = userTheme;
		update(stack === undefined ? rest : { ...rest, [key]: stack });
	};

	const update = (next: OqtoUiUserTheme) => {
		setJsonDraft(null);
		setJsonError(null);
		onChange(next);
	};

	const setOverride = (token: string, value: string) => {
		update({ ...userTheme, overrides: { ...overrides, [token]: value } });
	};

	const clearOverride = (token: string) => {
		const { [token]: _removed, ...rest } = overrides;
		update({ ...userTheme, overrides: rest });
	};

	const toggleOpen = () => {
		if (!open && themeRoot) {
			setBaseHex(snapshotTokenHex(themeRoot, COLOR_TOKENS));
		}
		setOpen((current) => !current);
	};

	const applyJson = () => {
		const parsed = parseUserThemeJson(jsonDraft ?? serialized);
		if (!parsed.ok) {
			setJsonError(parsed.error);
			return;
		}
		setJsonDraft(null);
		setJsonError(null);
		onChange(parsed.theme);
	};

	return (
		<div className="wb-theme-lab">
			<button
				aria-expanded={open}
				className="wb-theme-lab__toggle"
				type="button"
				onClick={toggleOpen}
			>
				<Palette aria-hidden="true" />
				<span>{t("oqtoUi.themeLab.label")}</span>
			</button>

			{open ? (
				<section
					aria-label={t("oqtoUi.themeLab.label")}
					className="wb-theme-lab__panel"
				>
					<header>
						<strong>{t("oqtoUi.themeLab.label")}</strong>
						<button
							aria-label={t("oqtoUi.themeLab.reset")}
							type="button"
							onClick={() => update({})}
						>
							<RotateCcw aria-hidden="true" />
						</button>
					</header>

					<ul className="wb-theme-lab__rows">
						{COLOR_TOKENS.map((token) => (
							<li key={token}>
								<label htmlFor={`wb-theme-${tokenKey(token)}`}>
									{t(`oqtoUi.themeLab.tokens.${tokenKey(token)}`)}
								</label>
								<input
									id={`wb-theme-${tokenKey(token)}`}
									type="color"
									value={
										overrides[token] ?? baseHex[token] ?? NEUTRAL_TOKEN_HEX
									}
									onChange={(event) => setOverride(token, event.target.value)}
								/>
								{overrides[token] ? (
									<button
										aria-label={t("oqtoUi.themeLab.clear", {
											token: t(`oqtoUi.themeLab.tokens.${tokenKey(token)}`),
										})}
										type="button"
										onClick={() => clearOverride(token)}
									>
										<X aria-hidden="true" />
									</button>
								) : null}
							</li>
						))}
						<li>
							<label htmlFor="wb-theme-radius">
								{t("oqtoUi.themeLab.radius")}
							</label>
							<input
								id="wb-theme-radius"
								max={20}
								min={0}
								type="range"
								value={radiusValue}
								onChange={(event) =>
									update({
										...userTheme,
										radius: `${event.target.value}px`,
									})
								}
							/>
							<span>
								{t("oqtoUi.themeLab.radiusValue", { value: radiusValue })}
							</span>
						</li>
						<li>
							<label htmlFor="wb-theme-shadows">
								{t("oqtoUi.themeLab.shadows")}
							</label>
							<select
								id="wb-theme-shadows"
								value={shadowPreset}
								onChange={(event) =>
									update(
										withShadowPreset(
											userTheme,
											event.target.value as ShadowPresetId,
										),
									)
								}
							>
								{SHADOW_PRESET_IDS.map((preset) => (
									<option key={preset} value={preset}>
										{t(`oqtoUi.themeLab.shadowPresets.${preset}`)}
									</option>
								))}
							</select>
						</li>
						<li>
							<label htmlFor="wb-theme-blur">{t("oqtoUi.themeLab.blur")}</label>
							<input
								id="wb-theme-blur"
								max={16}
								min={0}
								type="range"
								value={blurValue}
								onChange={(event) =>
									setOverride("--backdrop-blur", `${event.target.value}px`)
								}
							/>
							<span>
								{t("oqtoUi.themeLab.radiusValue", { value: blurValue })}
							</span>
						</li>
						<li>
							<label htmlFor="wb-theme-font-sans">
								{t("oqtoUi.themeLab.fontSans")}
							</label>
							<select
								id="wb-theme-font-sans"
								value={fontSansPreset}
								onChange={(event) =>
									setFont(
										"fontSans",
										FONT_SANS_PRESETS.find(
											(preset) => preset.id === event.target.value,
										)?.stack,
									)
								}
							>
								{FONT_SANS_PRESETS.map((preset) => (
									<option key={preset.id} value={preset.id}>
										{t(`oqtoUi.themeLab.fontPresets.${preset.id}`)}
									</option>
								))}
								{fontSansPreset === "custom" ? (
									<option value="custom">
										{t("oqtoUi.themeLab.fontPresets.custom")}
									</option>
								) : null}
							</select>
						</li>
						<li>
							<label htmlFor="wb-theme-font-mono">
								{t("oqtoUi.themeLab.fontMono")}
							</label>
							<select
								id="wb-theme-font-mono"
								value={fontMonoPreset}
								onChange={(event) =>
									setFont(
										"fontMono",
										FONT_MONO_PRESETS.find(
											(preset) => preset.id === event.target.value,
										)?.stack,
									)
								}
							>
								{FONT_MONO_PRESETS.map((preset) => (
									<option key={preset.id} value={preset.id}>
										{t(`oqtoUi.themeLab.fontPresets.${preset.id}`)}
									</option>
								))}
								{fontMonoPreset === "custom" ? (
									<option value="custom">
										{t("oqtoUi.themeLab.fontPresets.custom")}
									</option>
								) : null}
							</select>
						</li>
					</ul>

					<label className="wb-theme-lab__json-label" htmlFor="wb-theme-json">
						{t("oqtoUi.themeLab.jsonLabel")}
					</label>
					<textarea
						id="wb-theme-json"
						rows={7}
						spellCheck={false}
						value={jsonDraft ?? serialized}
						onChange={(event) => setJsonDraft(event.target.value)}
					/>
					{jsonError ? (
						<p className="wb-theme-lab__error" role="alert">
							{t("oqtoUi.themeLab.invalidJson", { message: jsonError })}
						</p>
					) : null}
					<button
						className="wb-theme-lab__apply"
						type="button"
						onClick={applyJson}
					>
						{t("oqtoUi.themeLab.applyJson")}
					</button>
				</section>
			) : null}
		</div>
	);
}
