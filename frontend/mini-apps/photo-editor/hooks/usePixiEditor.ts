import { useMountEffect } from "@/hooks/use-mount-effect";
import { Application, ColorMatrixFilter, Sprite, Texture } from "pixi.js";
import { useCallback, useRef } from "react";
import { applyAdjustments } from "../lib/color-matrix";
import {
	type Adjustments,
	DEFAULT_ADJUSTMENTS,
	type ImageInfo,
	type Viewport,
} from "../types";

interface Internals {
	app: Application | null;
	sprite: Sprite | null;
	filter: ColorMatrixFilter;
	source: HTMLImageElement | null;
	objectUrl: string | null;
	fitScale: number;
	adjustments: Adjustments;
}

export interface PixiEditorHandle {
	containerRef: React.RefObject<HTMLDivElement | null>;
	loadImageFromBlob: (blob: Blob, name: string) => Promise<ImageInfo | null>;
	setAdjustments: (adjustments: Adjustments) => void;
	setLayer: (visible: boolean, opacity: number) => void;
	getViewport: () => Viewport | null;
	getSource: () => HTMLImageElement | null;
	exportBlob: () => Promise<Blob | null>;
}

/**
 * Owns a PixiJS Application rendering a single image with a ColorMatrixFilter.
 * Everything is imperative: the only React effect is mount/unmount (creating and
 * destroying the Application). All edits are applied through the returned
 * methods, called from event handlers -- there are no reactive effects.
 */
export function usePixiEditor(): PixiEditorHandle {
	const containerRef = useRef<HTMLDivElement | null>(null);
	const internalsRef = useRef<Internals>({
		app: null,
		sprite: null,
		filter: new ColorMatrixFilter(),
		source: null,
		objectUrl: null,
		fitScale: 1,
		adjustments: { ...DEFAULT_ADJUSTMENTS },
	});

	const fit = useCallback(() => {
		const it = internalsRef.current;
		const app = it.app;
		const sprite = it.sprite;
		if (!app || !sprite) return;
		const cw = app.screen.width;
		const ch = app.screen.height;
		const iw = sprite.texture.width;
		const ih = sprite.texture.height;
		if (iw === 0 || ih === 0) return;
		const scale = Math.min(cw / iw, ch / ih);
		it.fitScale = scale;
		sprite.scale.set(scale);
		sprite.position.set(cw / 2, ch / 2);
	}, []);

	useMountEffect(() => {
		const container = containerRef.current;
		if (!container) return;
		const app = new Application();
		let cancelled = false;

		void app
			.init({
				resizeTo: container,
				backgroundAlpha: 0,
				antialias: true,
				resolution: 1,
			})
			.then(() => {
				if (cancelled) {
					app.destroy(true, { children: true });
					return;
				}
				container.appendChild(app.canvas);
				internalsRef.current.app = app;
				app.renderer.on("resize", fit);
				fit();
			});

		return () => {
			cancelled = true;
			const current = internalsRef.current.app;
			if (current) {
				current.renderer.off("resize", fit);
				current.destroy(true, { children: true });
				internalsRef.current.app = null;
				internalsRef.current.sprite = null;
			}
		};
	});

	const loadImageFromBlob = useCallback(
		async (blob: Blob, _name: string): Promise<ImageInfo | null> => {
			const it = internalsRef.current;
			const app = it.app;
			if (!app) return null;

			if (it.objectUrl) URL.revokeObjectURL(it.objectUrl);
			const url = URL.createObjectURL(blob);
			it.objectUrl = url;

			const img = new Image();
			img.src = url;
			await img.decode();
			it.source = img;

			const texture = Texture.from(img);
			if (!it.sprite) {
				const sprite = new Sprite(texture);
				sprite.anchor.set(0.5);
				sprite.filters = [it.filter];
				app.stage.addChild(sprite);
				it.sprite = sprite;
			} else {
				it.sprite.texture = texture;
			}
			applyAdjustments(it.filter, it.adjustments);
			fit();
			return {
				name: _name,
				width: img.naturalWidth,
				height: img.naturalHeight,
			};
		},
		[fit],
	);

	const setAdjustments = useCallback((adjustments: Adjustments) => {
		const it = internalsRef.current;
		it.adjustments = adjustments;
		applyAdjustments(it.filter, adjustments);
	}, []);

	const setLayer = useCallback((visible: boolean, opacity: number) => {
		const sprite = internalsRef.current.sprite;
		if (!sprite) return;
		sprite.visible = visible;
		sprite.alpha = opacity;
	}, []);

	const getViewport = useCallback((): Viewport | null => {
		const it = internalsRef.current;
		const app = it.app;
		const sprite = it.sprite;
		if (!app || !sprite) return null;
		const cw = app.screen.width;
		const ch = app.screen.height;
		const iw = sprite.texture.width;
		const ih = sprite.texture.height;
		const scale = it.fitScale;
		const dispW = iw * scale;
		const dispH = ih * scale;
		return {
			scale,
			offsetX: cw / 2 - dispW / 2,
			offsetY: ch / 2 - dispH / 2,
			imageWidth: iw,
			imageHeight: ih,
		};
	}, []);

	const getSource = useCallback(
		(): HTMLImageElement | null => internalsRef.current.source,
		[],
	);

	const exportBlob = useCallback(async (): Promise<Blob | null> => {
		const it = internalsRef.current;
		const app = it.app;
		const sprite = it.sprite;
		if (!app || !sprite) return null;
		const exportSprite = new Sprite(sprite.texture);
		exportSprite.filters = [it.filter];
		const canvas = app.renderer.extract.canvas({
			target: exportSprite,
		}) as HTMLCanvasElement;
		exportSprite.destroy();
		if (!canvas.toBlob) return null;
		return new Promise((resolve) => {
			canvas.toBlob((blob) => resolve(blob), "image/png");
		});
	}, []);

	return {
		containerRef,
		loadImageFromBlob,
		setAdjustments,
		setLayer,
		getViewport,
		getSource,
		exportBlob,
	};
}
