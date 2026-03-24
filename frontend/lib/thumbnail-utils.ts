/**
 * Thumbnail utilities for file browser
 */

export type ThumbnailSize = 128 | 256 | 512;

/**
 * Generate thumbnail URL for a file
 */
export function getThumbnailUrl({
	workspacePath,
	filePath,
	size = 256,
}: {
	workspacePath: string;
	filePath: string;
	size?: ThumbnailSize;
}): string {
	const params = new URLSearchParams({
		directory: workspacePath,
		path: filePath,
		size: size.toString(),
	});

	// The oqto-files server endpoint
	return `/api/files/thumbnail?${params.toString()}`;
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
