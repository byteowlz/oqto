import { useCallback, useState } from "react";
import {
	type OqtoUiSchemeId,
	type OqtoUiUserTheme,
	applyOqtoUiScheme,
	isOqtoUiSchemeId,
} from "../platform/base24-theme";

type ThemeRootBinding = {
	schemeId: OqtoUiSchemeId;
	rootRef: (root: HTMLDivElement | null) => void;
	rootEl: HTMLDivElement | null;
};

export function useThemeRoot(
	value: string,
	userTheme: OqtoUiUserTheme,
): ThemeRootBinding {
	const schemeId = isOqtoUiSchemeId(value) ? value : "oqto-dark";
	const [rootEl, setRootEl] = useState<HTMLDivElement | null>(null);
	const rootRef = useCallback(
		(root: HTMLDivElement | null) => {
			setRootEl(root);
			if (root) applyOqtoUiScheme(root, schemeId, userTheme);
		},
		[schemeId, userTheme],
	);
	return { schemeId, rootRef, rootEl };
}
