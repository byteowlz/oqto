import { useMountEffect } from "@/hooks/use-mount-effect";
import { useEffect, useState } from "react";

interface ShellLoadingState {
	mounted: boolean;
	shellReady: boolean;
	barVisible: boolean;
	barWidth: number;
	barFade: boolean;
}

export function useShellLoadingState(): ShellLoadingState {
	const [mounted, setMounted] = useState(false);
	const [shellReady, setShellReady] = useState(false);
	const [barVisible, setBarVisible] = useState(true);
	const [barWidth, setBarWidth] = useState(0);
	const [barFade, setBarFade] = useState(false);

	useMountEffect(() => {
		setMounted(true);
	});

	// useeffect-guardrail: allow - dependent on mounted state for post-mount fade-in sequence
	useEffect(() => {
		if (!mounted) return;
		const timer = requestAnimationFrame(() => {
			requestAnimationFrame(() => {
				setShellReady(true);
				document.getElementById("preload")?.remove();
				document.documentElement.removeAttribute("data-preload");
			});
		});
		return () => cancelAnimationFrame(timer);
	}, [mounted]);

	useMountEffect(() => {
		if (typeof window === "undefined") return;

		const applyViewportHeight = () => {
			const height = window.visualViewport?.height ?? window.innerHeight;
			document.documentElement.style.setProperty(
				"--app-viewport-height",
				`${height}px`,
			);
		};

		applyViewportHeight();
		window.visualViewport?.addEventListener("resize", applyViewportHeight);
		window.visualViewport?.addEventListener("scroll", applyViewportHeight);
		window.addEventListener("orientationchange", applyViewportHeight);
		window.addEventListener("pageshow", applyViewportHeight);
		document.addEventListener("visibilitychange", applyViewportHeight);

		setBarVisible(true);
		setBarFade(false);
		setBarWidth(25);
		const growTimer = window.setTimeout(() => setBarWidth(80), 150);
		const finish = () => {
			setBarWidth(100);
			setBarFade(true);
			window.setTimeout(() => setBarVisible(false), 500);
		};
		window.addEventListener("load", finish, { once: true });
		const fallback = window.setTimeout(finish, 1600);

		return () => {
			window.visualViewport?.removeEventListener("resize", applyViewportHeight);
			window.visualViewport?.removeEventListener("scroll", applyViewportHeight);
			window.removeEventListener("orientationchange", applyViewportHeight);
			window.removeEventListener("pageshow", applyViewportHeight);
			document.removeEventListener("visibilitychange", applyViewportHeight);
			window.clearTimeout(growTimer);
			window.clearTimeout(fallback);
			window.removeEventListener("load", finish);
		};
	});

	return {
		mounted,
		shellReady,
		barVisible,
		barWidth,
		barFade,
	};
}
