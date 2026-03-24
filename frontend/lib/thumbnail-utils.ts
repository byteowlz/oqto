/**
 * Thumbnail utilities for file browser
 */

import { readFileMux } from "@/lib/mux-files";

export type ThumbnailSize = 128 | 256 | 512;

// In-memory cache of blob URLs for thumbnails
const thumbnailBlobCache = new Map<string, string>();
const thumbnailInFlight = new Map<string, Promise<string | null>>();

function thumbnailCacheKey(workspacePath: string, filePath: string, size: ThumbnailSize): string {
	return `${workspacePath}:${filePath}:${size}`;
}

/**
 * Fetch a thumbnail via the mux layer and return a blob URL.
 * Results are cached in memory. Returns null on failure.
 */
export async function fetchThumbnailUrl({
	workspacePath,
	filePath,
	size = 256,
}: {
	workspacePath: string;
	filePath: string;
	size?: ThumbnailSize;
}): Promise<string | null> {
	const key = thumbnailCacheKey(workspacePath, filePath, size);

	// Check cache
	const cached = thumbnailBlobCache.get(key);
	if (cached) return cached;

	// Deduplicate in-flight requests
	const existing = thumbnailInFlight.get(key);
	if (existing) return existing;

	const request = (async (): Promise<string | null> => {
		try {
			const result = await readFileMux(workspacePath, filePath);
			if (!result.data || result.data.byteLength === 0) return null;

			// Guess MIME type from extension
			const ext = filePath.substring(filePath.lastIndexOf(".")).toLowerCase();
			const mimeTypes: Record<string, string> = {
				".png": "image/png",
				".jpg": "image/jpeg",
				".jpeg": "image/jpeg",
				".gif": "image/gif",
				".webp": "image/webp",
				".svg": "image/svg+xml",
				".bmp": "image/bmp",
				".ico": "image/x-icon",
				".mp4": "video/mp4",
				".webm": "video/webm",
				".ogg": "video/ogg",
				".mov": "video/quicktime",
			};
			const mime = mimeTypes[ext] ?? "application/octet-stream";

			const blob = new Blob([result.data], { type: mime });
			const url = URL.createObjectURL(blob);
			thumbnailBlobCache.set(key, url);
			return url;
		} catch (err) {
			console.warn("[thumbnail] Failed to fetch:", filePath, err);
			return null;
		} finally {
			thumbnailInFlight.delete(key);
		}
	})();

	thumbnailInFlight.set(key, request);
	return request;
}

/**
 * Clear cached thumbnail blob URLs (call on workspace change)
 */
export function clearThumbnailCache(workspacePath?: string): void {
	if (workspacePath) {
		const prefix = `${workspacePath}:`;
		for (const [key, url] of thumbnailBlobCache.entries()) {
			if (key.startsWith(prefix)) {
				URL.revokeObjectURL(url);
				thumbnailBlobCache.delete(key);
			}
		}
	} else {
		for (const url of thumbnailBlobCache.values()) {
			URL.revokeObjectURL(url);
		}
		thumbnailBlobCache.clear();
	}
}

/**
 * Check if a file extension supports thumbnails (images only)
 */
export function supportsThumbnail(filename: string): boolean {
	const ext = filename.substring(filename.lastIndexOf(".")).toLowerCase();
	const supportedExtensions = new Set([
		".png",
		".jpg",
		".jpeg",
		".gif",
		".webp",
		".svg",
		".bmp",
		".ico",
	]);

	return supportedExtensions.has(ext);
}

/**
 * Check if a file supports thumbnail generation (images + videos)
 */
export function supportsMediaThumbnail(filename: string): boolean {
	return supportsThumbnail(filename) || isVideoFile(filename);
}

/**
 * Check if a file is a video (for future video thumbnail support)
 */
export function isVideoFile(filename: string): boolean {
	const ext = filename.substring(filename.lastIndexOf(".")).toLowerCase();
	const videoExtensions = new Set([
		".mp4",
		".webm",
		".ogg",
		".ogv",
		".mov",
		".avi",
		".mkv",
		".m4v",
	]);

	return videoExtensions.has(ext);
}

/**
 * Format duration in seconds to MM:SS
 */
export function formatDuration(seconds: number): string {
	const mins = Math.floor(seconds / 60);
	const secs = Math.floor(seconds % 60);
	return `${mins}:${secs.toString().padStart(2, "0")}`;
}
