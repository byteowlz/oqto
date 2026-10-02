// App presentations receive the active host mode, never a guess from the
// ordering of a CSS `color-scheme: light dark` declaration.
export function detectAppColorScheme(root: HTMLElement): "light" | "dark" {
	if (root.classList.contains("dark")) return "dark";
	if (root.classList.contains("light")) return "light";
	const dataTheme = root.getAttribute("data-theme");
	if (dataTheme === "dark" || dataTheme === "light") return dataTheme;

	const schemes = getComputedStyle(root).colorScheme.toLowerCase().split(/\s+/);
	const dark = schemes.includes("dark");
	const light = schemes.includes("light");
	if (dark && !light) return "dark";
	if (light && !dark) return "light";
	return window.matchMedia?.("(prefers-color-scheme: dark)").matches
		? "dark"
		: "light";
}

/** Observe both explicit host changes and system-mode changes, with teardown. */
export function observeAppTheme(
	root: HTMLElement,
	notify: () => void,
): () => void {
	const observer = new MutationObserver(notify);
	observer.observe(root, {
		attributes: true,
		attributeFilter: ["class", "style", "data-theme"],
	});
	const preference = window.matchMedia?.("(prefers-color-scheme: dark)");
	preference?.addEventListener("change", notify);
	return () => {
		observer.disconnect();
		preference?.removeEventListener("change", notify);
	};
}
