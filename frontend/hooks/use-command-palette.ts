"use client";

import { useEffect, useState } from "react";

export function useCommandPalette() {
	const [open, setOpen] = useState(false);

	useEffect(() => {
		const down = (e: KeyboardEvent) => {
			// Cmd+K on Mac, Ctrl+K on Windows/Linux
			if (e.key === "k" && (e.metaKey || e.ctrlKey)) {
				e.preventDefault();
				setOpen((prev) => !prev);
			}
		};

		document.addEventListener("keydown", down);
		return () => document.removeEventListener("keydown", down);
	}, []);

	return { open, setOpen };
}
