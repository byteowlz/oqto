import {
	type Scheme,
	applyScheme,
	nordBase16,
	nordLight,
	oqtoDark,
	oqtoLight,
} from "@byteowlz/design-system";

export type WorkbenchSchemeId =
	| "oqto-dark"
	| "oqto-light"
	| "nord-dark"
	| "nord-light";

export type WorkbenchUserTheme = {
	overrides?: Record<string, string>;
	radius?: string;
	fontSans?: string;
	fontMono?: string;
};

const schemes: Readonly<Record<WorkbenchSchemeId, Scheme>> = {
	"oqto-dark": oqtoDark,
	"oqto-light": oqtoLight,
	"nord-dark": { ...nordBase16, id: "nord-dark", name: "Nord Dark" },
	"nord-light": nordLight,
};

export function isWorkbenchSchemeId(value: string): value is WorkbenchSchemeId {
	return value in schemes;
}

function hasUserTheme(userTheme: WorkbenchUserTheme): boolean {
	return (
		Object.keys(userTheme.overrides ?? {}).length > 0 ||
		userTheme.radius !== undefined ||
		userTheme.fontSans !== undefined ||
		userTheme.fontMono !== undefined
	);
}

export function applyWorkbenchScheme(
	root: HTMLElement,
	schemeId: WorkbenchSchemeId,
	userTheme: WorkbenchUserTheme = {},
): void {
	const base = schemes[schemeId];
	const scheme: Scheme = hasUserTheme(userTheme)
		? { ...base, overrides: { ...base.overrides, ...userTheme.overrides } }
		: base;
	applyScheme(scheme, {
		root,
		radius: userTheme.radius,
		identity: {
			...(userTheme.fontSans !== undefined && { fontSans: userTheme.fontSans }),
			...(userTheme.fontMono !== undefined && { fontMono: userTheme.fontMono }),
		},
	});
	root.dataset.scheme = schemeId;
	root.dataset.themeSource = hasUserTheme(userTheme) ? "user" : "scheme";
}

export type ParsedUserTheme =
	| { ok: true; theme: WorkbenchUserTheme }
	| { ok: false; error: string };

/** Fail-loud parse of the agent-authorable user-theme JSON. */
export function parseUserThemeJson(text: string): ParsedUserTheme {
	let raw: unknown;
	try {
		raw = JSON.parse(text);
	} catch (cause) {
		return { ok: false, error: (cause as Error).message };
	}
	if (typeof raw !== "object" || raw === null || Array.isArray(raw)) {
		return { ok: false, error: "expected an object" };
	}
	const record = raw as {
		radius?: unknown;
		overrides?: unknown;
		fontSans?: unknown;
		fontMono?: unknown;
	};
	const theme: WorkbenchUserTheme = {};
	if (record.radius !== undefined) {
		if (typeof record.radius !== "string") {
			return { ok: false, error: "radius must be a CSS length string" };
		}
		theme.radius = record.radius;
	}
	for (const key of ["fontSans", "fontMono"] as const) {
		if (record[key] !== undefined) {
			if (typeof record[key] !== "string") {
				return { ok: false, error: `${key} must be a font stack string` };
			}
			theme[key] = record[key];
		}
	}
	if (record.overrides !== undefined) {
		if (
			typeof record.overrides !== "object" ||
			record.overrides === null ||
			Array.isArray(record.overrides)
		) {
			return { ok: false, error: "overrides must be an object" };
		}
		const overrides: Record<string, string> = {};
		for (const [key, value] of Object.entries(record.overrides)) {
			if (!key.startsWith("--")) {
				return { ok: false, error: `override "${key}" must start with --` };
			}
			if (typeof value !== "string") {
				return { ok: false, error: `override "${key}" must be a string` };
			}
			overrides[key] = value;
		}
		theme.overrides = overrides;
	}
	return { ok: true, theme };
}
