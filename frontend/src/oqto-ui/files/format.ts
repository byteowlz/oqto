/**
 * Human-readable entry facts. Pure formatting: the pane passes translated
 * templates in, so no user-facing string is built here.
 */

export interface SizeLabels {
	readonly bytes: (count: number) => string;
	readonly kilo: (value: string) => string;
	readonly mega: (value: string) => string;
	readonly giga: (value: string) => string;
}

const KILO = 1024;

export function formatSize(size: number, labels: SizeLabels): string {
	if (size < KILO) return labels.bytes(size);
	if (size < KILO ** 2)
		return labels.kilo((size / KILO).toFixed(size < 10 * KILO ? 1 : 0));
	if (size < KILO ** 3) return labels.mega((size / KILO ** 2).toFixed(1));
	return labels.giga((size / KILO ** 3).toFixed(1));
}

/**
 * Short, scannable timestamps: time of day for today, day and month for
 * this year, year otherwise. `now` is passed in so the result is testable.
 */
export function formatModified(
	modifiedAt: number,
	locale: string,
	now: number,
): string {
	if (modifiedAt <= 0) return "";
	const date = new Date(modifiedAt);
	const today = new Date(now);
	const sameDay =
		date.getFullYear() === today.getFullYear() &&
		date.getMonth() === today.getMonth() &&
		date.getDate() === today.getDate();
	if (sameDay) {
		return date.toLocaleTimeString(locale, {
			hour: "2-digit",
			minute: "2-digit",
		});
	}
	if (date.getFullYear() === today.getFullYear()) {
		return date.toLocaleDateString(locale, { day: "2-digit", month: "short" });
	}
	return date.toLocaleDateString(locale, { year: "numeric", month: "short" });
}
