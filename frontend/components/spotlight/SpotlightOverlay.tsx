"use client";

import {
	type CSSProperties,
	useCallback,
	useEffect,
	useLayoutEffect,
	useMemo,
	useRef,
	useState,
} from "react";

export type SpotlightPosition = "auto" | "top" | "bottom" | "left" | "right";

export interface SpotlightState {
	active: boolean;
	target: string;
	title?: string;
	description?: string;
	action?: string;
	position?: SpotlightPosition;
}

export interface SpotlightTour {
	active: boolean;
	steps: SpotlightState[];
	index: number;
}

interface SpotlightOverlayProps {
	spotlight: SpotlightState | null;
	tour: SpotlightTour | null;
	onClose: () => void;
	onNext?: () => void;
	onPrev?: () => void;
}

interface SpotlightRect {
	x: number;
	y: number;
	width: number;
	height: number;
	radius: number;
}

export function SpotlightOverlay({
	spotlight,
	tour,
	onClose,
	onNext,
	onPrev,
}: SpotlightOverlayProps) {
	const [targetRect, setTargetRect] = useState<SpotlightRect | null>(null);
	const [tooltipStyle, setTooltipStyle] = useState<CSSProperties | null>(null);
	const tooltipRef = useRef<HTMLDivElement | null>(null);

	const activeSpotlight = spotlight?.active ? spotlight : null;

	const updateTargetRect = useCallback(() => {
		if (!activeSpotlight?.target) {
			setTargetRect(null);
			return;
		}
		const element = document.querySelector<HTMLElement>(
			`[data-spotlight="${activeSpotlight.target}"]`,
		);
		if (!element) {
			setTargetRect(null);
			return;
		}
		const rect = element.getBoundingClientRect();
		const padding = 6;
		setTargetRect({
			x: rect.left - padding,
			y: rect.top - padding,
			width: rect.width + padding * 2,
			height: rect.height + padding * 2,
			radius: Math.min(12, rect.height / 4),
		});
	}, [activeSpotlight?.target]);

	useLayoutEffect(() => {
		updateTargetRect();
	}, [updateTargetRect]);

	useEffect(() => {
		if (!activeSpotlight?.target) return;
		const handle = () => updateTargetRect();
		window.addEventListener("resize", handle);
		window.addEventListener("scroll", handle, true);
		return () => {
			window.removeEventListener("resize", handle);
			window.removeEventListener("scroll", handle, true);
		};
	}, [activeSpotlight?.target, updateTargetRect]);

	useLayoutEffect(() => {
		if (!targetRect || !tooltipRef.current) {
			setTooltipStyle(null);
			return;
		}
		const tooltip = tooltipRef.current.getBoundingClientRect();
		const margin = 12;
		const viewportW = window.innerWidth;
		const viewportH = window.innerHeight;

		const positions: Array<{
			position: SpotlightPosition;
			style: CSSProperties;
		}> = [
			{
				position: "top",
				style: {
					left: Math.min(
						viewportW - tooltip.width - margin,
						Math.max(
							margin,
							targetRect.x + targetRect.width / 2 - tooltip.width / 2,
						),
					),
					top: Math.max(margin, targetRect.y - tooltip.height - margin),
				},
			},
			{
				position: "bottom",
				style: {
					left: Math.min(
						viewportW - tooltip.width - margin,
						Math.max(
							margin,
							targetRect.x + targetRect.width / 2 - tooltip.width / 2,
						),
					),
					top: Math.min(
						viewportH - tooltip.height - margin,
						targetRect.y + targetRect.height + margin,
					),
				},
			},
			{
				position: "right",
				style: {
					left: Math.min(
						viewportW - tooltip.width - margin,
						targetRect.x + targetRect.width + margin,
					),
					top: Math.min(
						viewportH - tooltip.height - margin,
						Math.max(
							margin,
							targetRect.y + targetRect.height / 2 - tooltip.height / 2,
						),
					),
				},
			},
			{
				position: "left",
				style: {
					left: Math.max(margin, targetRect.x - tooltip.width - margin),
					top: Math.min(
						viewportH - tooltip.height - margin,
						Math.max(
							margin,
							targetRect.y + targetRect.height / 2 - tooltip.height / 2,
						),
					),
				},
			},
		];

		const desired = activeSpotlight?.position ?? "auto";
		let chosen = positions[0];

		if (desired !== "auto") {
			const explicit = positions.find((p) => p.position === desired);
			if (explicit) {
				chosen = explicit;
			}
		} else {
			chosen =
				positions.find(
					(p) =>
						p.style.top !== undefined &&
						p.style.left !== undefined &&
						p.style.top >= margin &&
						p.style.left >= margin &&
						p.style.top + tooltip.height + margin <= viewportH &&
						p.style.left + tooltip.width + margin <= viewportW,
				) ?? positions[0];
		}

		setTooltipStyle({
			position: "fixed",
			left: chosen.style.left,
			top: chosen.style.top,
		});
	}, [activeSpotlight?.position, targetRect]);

	const maskId = useMemo(
		() => `spotlight-mask-${activeSpotlight?.target ?? "none"}`,
		[activeSpotlight?.target],
	);

	if (!activeSpotlight || !targetRect) return null;

	const tourBadge = tour
		? `Step ${tour.index + 1} of ${tour.steps.length}`
		: null;

	return (
		<div className="fixed inset-0 z-[9999] pointer-events-none">
			<svg className="absolute inset-0 h-full w-full pointer-events-none">
				<title>Spotlight overlay</title>
				<defs>
					<mask id={maskId}>
						<rect width="100%" height="100%" fill="white" />
						<rect
							x={targetRect.x}
							y={targetRect.y}
							width={targetRect.width}
							height={targetRect.height}
							rx={targetRect.radius}
							ry={targetRect.radius}
							fill="black"
						/>
					</mask>
				</defs>
				<rect
					width="100%"
					height="100%"
					fill="rgba(0, 0, 0, 0.55)"
					mask={`url(#${maskId})`}
				/>
			</svg>

			<div
				className="absolute rounded-lg pointer-events-none ring-2 ring-primary/70 shadow-[0_0_0_6px_rgba(255,255,255,0.05)] animate-pulse"
				style={{
					left: targetRect.x,
					top: targetRect.y,
					width: targetRect.width,
					height: targetRect.height,
				}}
			/>

			<div
				ref={tooltipRef}
				className="pointer-events-auto max-w-xs rounded-lg border border-border bg-popover p-4 text-sm text-popover-foreground shadow-xl"
				style={tooltipStyle ?? { position: "fixed", left: 16, top: 16 }}
			>
				<div className="flex items-start justify-between gap-2">
					<div className="space-y-2">
						{tourBadge && (
							<div className="text-[11px] uppercase tracking-wider text-muted-foreground">
								{tourBadge}
							</div>
						)}
						{activeSpotlight.title && (
							<div className="text-sm font-semibold">
								{activeSpotlight.title}
							</div>
						)}
						{activeSpotlight.description && (
							<div className="text-xs text-muted-foreground">
								{activeSpotlight.description}
							</div>
						)}
					</div>
					<button
						type="button"
						onClick={onClose}
						className="text-muted-foreground hover:text-foreground"
						aria-label="Close spotlight"
					>
						x
					</button>
				</div>

				{activeSpotlight.action && (
					<div className="mt-3 text-xs text-muted-foreground">
						{activeSpotlight.action}
					</div>
				)}

				{tour && (
					<div className="mt-4 flex items-center justify-between gap-2 text-xs">
						<button
							type="button"
							onClick={onPrev}
							disabled={tour.index === 0}
							className="rounded border border-border px-2 py-1 disabled:opacity-50"
						>
							Back
						</button>
						<button
							type="button"
							onClick={onNext}
							disabled={tour.index >= tour.steps.length - 1}
							className="rounded bg-primary px-2 py-1 text-primary-foreground disabled:opacity-50"
						>
							Next
						</button>
					</div>
				)}
			</div>
		</div>
	);
}
