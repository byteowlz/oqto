/**
 * Avatar utilities for handling different avatar sources.
 *
 * Avatar sources:
 * 1. System avatars: "/avatars/developer.png" - served from frontend/public
 * 2. Persona-local: "avatar.png" - relative to persona directory, served via fileserver
 * 3. User avatars: "user://abc123.png" - stored in ~/octo/avatars/, served via fileserver
 */

/**
 * Avatar specifications
 */
export const AVATAR_SPECS = {
	/** Recommended width and height in pixels */
	size: 256,
	/** Aspect ratio (width / height) */
	aspectRatio: 1,
	/** Supported formats */
	formats: ["image/png", "image/webp", "image/jpeg"] as const,
	/** Max file size in bytes (1MB) */
	maxFileSize: 1024 * 1024,
};

/**
 * System avatars that ship with octo.
 * These are available at /avatars/{name}.png
 */
export const SYSTEM_AVATARS = [
	{ id: "default", name: "Default", path: "/avatars/default.png" },
	{ id: "developer", name: "Developer", path: "/avatars/developer.png" },
	{ id: "researcher", name: "Researcher", path: "/avatars/researcher.png" },
	{ id: "writer", name: "Writer", path: "/avatars/writer.png" },
	{ id: "analyst", name: "Analyst", path: "/avatars/analyst.png" },
	{ id: "creative", name: "Creative", path: "/avatars/creative.png" },
	{ id: "assistant", name: "Assistant", path: "/avatars/assistant.png" },
] as const;

export type SystemAvatarId = (typeof SYSTEM_AVATARS)[number]["id"];

/**
 * Check if an avatar path is a system avatar.
 */
export function isSystemAvatar(avatarPath: string | null | undefined): boolean {
	if (!avatarPath) return false;
	return avatarPath.startsWith("/avatars/");
}

/**
 * Check if an avatar path is a user-generated avatar.
 */
export function isUserAvatar(avatarPath: string | null | undefined): boolean {
	if (!avatarPath) return false;
	return avatarPath.startsWith("user://");
}

/**
 * Resolve an avatar path to a URL that can be used in an img src.
 *
 * @param avatarPath - The avatar path from persona.toml
 * @param sessionId - The workspace session ID (needed for fileserver URLs)
 * @returns The resolved URL or null if no avatar
 */
export function resolveAvatarUrl(
	avatarPath: string | null | undefined,
	sessionId?: string,
): string | null {
	if (!avatarPath) return null;

	// System avatar - served from frontend public
	if (isSystemAvatar(avatarPath)) {
		return avatarPath;
	}

	// User avatar - served via fileserver from ~/octo/avatars/
	if (isUserAvatar(avatarPath)) {
		if (!sessionId) return null;
		const filename = avatarPath.replace("user://", "");
		// Assumes fileserver has access to ~/octo/avatars/
		return `/api/session/${sessionId}/files/file?path=${encodeURIComponent(`avatars/${filename}`)}`;
	}

	// Persona-local avatar - served via fileserver from persona directory
	if (sessionId) {
		return `/api/session/${sessionId}/files/file?path=${encodeURIComponent(avatarPath)}`;
	}

	return null;
}

/**
 * Get the default avatar URL for a persona based on its role/type.
 * Uses color as a fallback indicator if no specific avatar is set.
 */
export function getDefaultAvatarUrl(personaId: string): string {
	// Try to match persona ID to a system avatar
	const systemAvatar = SYSTEM_AVATARS.find(
		(a) => a.id === personaId || personaId.toLowerCase().includes(a.id),
	);

	if (systemAvatar) {
		return systemAvatar.path;
	}

	return "/avatars/default.png";
}
