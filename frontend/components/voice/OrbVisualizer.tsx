/**
 * Orb Visualizer - 3D shader-based voice visualization.
 *
 * A fluid, organic orb that responds to input/output volume levels.
 * Uses Three.js with procedural noise for animation.
 *
 * Note: This is a simplified version that doesn't require external textures.
 * For the full version with Perlin noise texture, see K.I.T.T. repo.
 */

"use client";

import { cn } from "@/lib/utils";
import type { VoiceState } from "@/lib/voice/types";
import { useEffect, useMemo, useRef, useState } from "react";

// Check if we're in a browser environment with WebGL support
const hasWebGL =
	typeof window !== "undefined" &&
	!!document.createElement("canvas").getContext("webgl2");

export interface OrbVisualizerProps {
	/** Current voice state */
	state: VoiceState;
	/** Input volume (0-1) */
	inputVolume: number;
	/** Output volume (0-1) */
	outputVolume: number;
	/** Primary color */
	color1?: string;
	/** Secondary color */
	color2?: string;
	/** Optional class name */
	className?: string;
}

/**
 * Orb visualizer using Canvas 2D fallback.
 * For a full WebGL version, install @react-three/fiber and @react-three/drei.
 */
export function OrbVisualizer({
	state,
	inputVolume,
	outputVolume,
	color1 = "#60a5fa", // blue-400
	color2 = "#a78bfa", // violet-400
	className,
}: OrbVisualizerProps) {
	const canvasRef = useRef<HTMLCanvasElement>(null);
	const animationRef = useRef<number | null>(null);
	const timeRef = useRef(0);

	// State-based colors
	const colors = useMemo(() => {
		switch (state) {
			case "listening":
				return { c1: "#3b82f6", c2: "#60a5fa" }; // Blue
			case "processing":
				return { c1: "#8b5cf6", c2: "#a78bfa" }; // Purple
			case "speaking":
				return { c1: "#22c55e", c2: "#4ade80" }; // Green
			default:
				return { c1: color1, c2: color2 };
		}
	}, [state, color1, color2]);

	useEffect(() => {
		const canvas = canvasRef.current;
		if (!canvas) return;

		const ctx = canvas.getContext("2d");
		if (!ctx) return;

		// Handle resize
		const resize = () => {
			const dpr = window.devicePixelRatio || 1;
			const rect = canvas.getBoundingClientRect();
			canvas.width = rect.width * dpr;
			canvas.height = rect.height * dpr;
			ctx.scale(dpr, dpr);
		};

		resize();
		window.addEventListener("resize", resize);

		// Animation loop
		const draw = () => {
			const rect = canvas.getBoundingClientRect();
			const width = rect.width;
			const height = rect.height;
			const centerX = width / 2;
			const centerY = height / 2;
			const baseRadius = Math.min(width, height) * 0.35;

			// Clear
			ctx.clearRect(0, 0, width, height);

			// Update time
			timeRef.current += 0.016;
			const t = timeRef.current;

			// Calculate animated radius based on volume
			const activeVolume = state === "speaking" ? outputVolume : inputVolume;
			const volumeScale = 1 + activeVolume * 0.3;
			const breathe = 1 + Math.sin(t * 2) * 0.02;
			const radius = baseRadius * volumeScale * breathe;

			// Create gradient
			const gradient = ctx.createRadialGradient(
				centerX - radius * 0.2,
				centerY - radius * 0.2,
				0,
				centerX,
				centerY,
				radius * 1.2,
			);
			gradient.addColorStop(0, colors.c2);
			gradient.addColorStop(0.5, colors.c1);
			gradient.addColorStop(1, "transparent");

			// Draw main orb with glow
			ctx.save();

			// Outer glow
			ctx.beginPath();
			ctx.arc(centerX, centerY, radius * 1.3, 0, Math.PI * 2);
			const glowGradient = ctx.createRadialGradient(
				centerX,
				centerY,
				radius * 0.8,
				centerX,
				centerY,
				radius * 1.3,
			);
			glowGradient.addColorStop(0, `${colors.c1}40`);
			glowGradient.addColorStop(1, "transparent");
			ctx.fillStyle = glowGradient;
			ctx.fill();

			// Main orb
			ctx.beginPath();
			ctx.arc(centerX, centerY, radius, 0, Math.PI * 2);
			ctx.fillStyle = gradient;
			ctx.fill();

			// Inner rings (volume reactive)
			const numRings = 5;
			for (let i = 0; i < numRings; i++) {
				const ringProgress = i / numRings;
				const ringRadius = radius * (0.3 + ringProgress * 0.6);
				const ringOpacity = 0.1 + inputVolume * 0.2;
				const waveOffset = Math.sin(t * 3 + i * 0.5) * activeVolume * 5;

				ctx.beginPath();
				ctx.arc(centerX, centerY, ringRadius + waveOffset, 0, Math.PI * 2);
				ctx.strokeStyle = `rgba(255, 255, 255, ${ringOpacity})`;
				ctx.lineWidth = 1 + activeVolume * 2;
				ctx.stroke();
			}

			// Animated particles
			const numParticles = Math.floor(8 + outputVolume * 12);
			for (let i = 0; i < numParticles; i++) {
				const angle = (i / numParticles) * Math.PI * 2 + t * 0.5;
				const particleRadius = radius * (0.6 + Math.sin(t * 2 + i) * 0.3);
				const px = centerX + Math.cos(angle) * particleRadius;
				const py = centerY + Math.sin(angle) * particleRadius;
				const particleSize = 2 + activeVolume * 3;

				ctx.beginPath();
				ctx.arc(px, py, particleSize, 0, Math.PI * 2);
				ctx.fillStyle = `rgba(255, 255, 255, ${0.3 + activeVolume * 0.4})`;
				ctx.fill();
			}

			ctx.restore();

			animationRef.current = requestAnimationFrame(draw);
		};

		draw();

		return () => {
			window.removeEventListener("resize", resize);
			if (animationRef.current) {
				cancelAnimationFrame(animationRef.current);
			}
		};
	}, [state, inputVolume, outputVolume, colors]);

	return (
		<div className={cn("relative w-full h-full min-h-[200px]", className)}>
			<canvas
				ref={canvasRef}
				className="w-full h-full"
				style={{ display: "block" }}
			/>

			{/* State indicator */}
			<div className="absolute bottom-4 left-1/2 -translate-x-1/2">
				<div
					className={cn(
						"px-3 py-1 rounded-full text-xs font-medium",
						"bg-black/50 backdrop-blur-sm",
						state === "listening" && "text-blue-400",
						state === "processing" && "text-purple-400",
						state === "speaking" && "text-green-400",
						state === "idle" && "text-gray-400",
					)}
				>
					{state === "idle"
						? "Ready"
						: state.charAt(0).toUpperCase() + state.slice(1)}
				</div>
			</div>
		</div>
	);
}
