"use client";

import { useEffect, useState } from "react";

const BRAILLE_PATTERNS = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

/**
 * 6-dot braille spinner - cycles through braille patterns.
 */
export function BrailleSpinner() {
	const [frame, setFrame] = useState(0);

	useEffect(() => {
		const interval = setInterval(() => {
			setFrame((f) => (f + 1) % BRAILLE_PATTERNS.length);
		}, 80);
		return () => clearInterval(interval);
	}, []);

	return (
		<span className="text-primary font-mono text-sm">
			{BRAILLE_PATTERNS[frame]}
		</span>
	);
}
