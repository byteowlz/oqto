import { useCallback, useState } from "react";
import {
	type WorkbenchSchemeId,
	type WorkbenchUserTheme,
	applyWorkbenchScheme,
	isWorkbenchSchemeId,
} from "../../adapters/theme/base24-theme";

type ThemeRootBinding = {
	schemeId: WorkbenchSchemeId;
	rootRef: (root: HTMLDivElement | null) => void;
	rootEl: HTMLDivElement | null;
};

export function useThemeRoot(
	value: string,
	userTheme: WorkbenchUserTheme,
): ThemeRootBinding {
	const schemeId = isWorkbenchSchemeId(value) ? value : "oqto-dark";
	const [rootEl, setRootEl] = useState<HTMLDivElement | null>(null);
	const rootRef = useCallback(
		(root: HTMLDivElement | null) => {
			setRootEl(root);
			if (root) applyWorkbenchScheme(root, schemeId, userTheme);
		},
		[schemeId, userTheme],
	);
	return { schemeId, rootRef, rootEl };
}
