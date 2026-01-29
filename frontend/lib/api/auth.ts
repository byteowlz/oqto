/**
 * Authentication API
 * Login, logout, register, current user
 */

import {
	authFetch,
	controlPlaneApiUrl,
	getAuthHeaders,
	readApiError,
	setAuthToken,
} from "./client";
import type {
	LoginRequest,
	LoginResponse,
	RegisterRequest,
	RegisterResponse,
	UserInfo,
} from "./types";

export async function login(request: LoginRequest): Promise<LoginResponse> {
	const url = controlPlaneApiUrl("/api/auth/login");
	const options: RequestInit = {
		method: "POST",
		headers: { "Content-Type": "application/json" },
		body: JSON.stringify(request),
		credentials: "include",
	};

	// Retry logic for transient network errors (e.g., ERR_CONNECTION_REFUSED on first attempt)
	const maxRetries = 2;
	let lastError: Error | undefined;

	for (let attempt = 0; attempt <= maxRetries; attempt++) {
		try {
			const res = await fetch(url, options);
			if (!res.ok) throw new Error(await readApiError(res));
			const data: LoginResponse = await res.json();
			// Store token for Tauri/mobile
			if (data.token) {
				setAuthToken(data.token);
			}
			return data;
		} catch (error) {
			lastError = error instanceof Error ? error : new Error(String(error));
			// Only retry on network errors, not HTTP errors
			const isNetworkError =
				lastError.message.includes("Failed to fetch") ||
				lastError.message.includes("NetworkError") ||
				lastError.message.includes("network") ||
				lastError.name === "TypeError"; // fetch throws TypeError on network failure

			if (!isNetworkError || attempt === maxRetries) {
				throw lastError;
			}

			// Wait before retrying (50ms, then 100ms)
			await new Promise((resolve) => setTimeout(resolve, 50 * (attempt + 1)));
		}
	}

	throw lastError ?? new Error("Login failed");
}

export async function register(
	request: RegisterRequest,
): Promise<RegisterResponse> {
	const url = controlPlaneApiUrl("/api/auth/register");
	const options: RequestInit = {
		method: "POST",
		headers: { "Content-Type": "application/json" },
		body: JSON.stringify(request),
		credentials: "include",
	};

	// Retry logic for transient network errors
	const maxRetries = 2;
	let lastError: Error | undefined;

	for (let attempt = 0; attempt <= maxRetries; attempt++) {
		try {
			const res = await fetch(url, options);
			if (!res.ok) throw new Error(await readApiError(res));
			const data: RegisterResponse = await res.json();
			// Store token for Tauri/mobile
			if (data.token) {
				setAuthToken(data.token);
			}
			return data;
		} catch (error) {
			lastError = error instanceof Error ? error : new Error(String(error));
			// Only retry on network errors, not HTTP errors
			const isNetworkError =
				lastError.message.includes("Failed to fetch") ||
				lastError.message.includes("NetworkError") ||
				lastError.message.includes("network") ||
				lastError.name === "TypeError";

			if (!isNetworkError || attempt === maxRetries) {
				throw lastError;
			}

			// Wait before retrying
			await new Promise((resolve) => setTimeout(resolve, 50 * (attempt + 1)));
		}
	}

	throw lastError ?? new Error("Registration failed");
}

export async function logout(): Promise<void> {
	const res = await authFetch(controlPlaneApiUrl("/api/auth/logout"), {
		method: "POST",
		headers: { ...getAuthHeaders() },
		credentials: "include",
	});
	// Clear token regardless of response
	setAuthToken(null);
	if (!res.ok) throw new Error(await readApiError(res));
}

export async function getCurrentUser(): Promise<UserInfo | null> {
	const res = await authFetch(controlPlaneApiUrl("/api/me"), {
		credentials: "include",
	});
	if (res.status === 401) return null;
	if (!res.ok) throw new Error(await readApiError(res));
	return res.json();
}

/** @deprecated Use login() instead */
export async function devLogin(): Promise<boolean> {
	try {
		await login({ username: "dev", password: "devpassword123" });
		return true;
	} catch {
		return false;
	}
}
