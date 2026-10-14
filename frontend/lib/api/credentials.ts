import type { CredentialCommand } from "@/src/generated/CredentialCommand";
import type { CredentialManageResult } from "@/src/generated/CredentialManageResult";
import { authFetch, controlPlaneApiUrl } from "./client";

/** Actual transport URLs, never isSecureContext (which can be overridden). */
export function isCredentialTransportSafe(url: string): boolean {
	try {
		const parsed = new URL(url);
		return (
			parsed.protocol === "https:" ||
			(parsed.protocol === "http:" &&
				(parsed.hostname === "localhost" ||
					parsed.hostname === "[::1]" ||
					/^127\.(\d{1,3}\.){2}\d{1,3}$/.test(parsed.hostname)))
		);
	} catch {
		return false;
	}
}

export function canProvisionCredentials(): boolean {
	if (typeof window === "undefined") return false;
	return (
		isCredentialTransportSafe(window.location.href) &&
		isCredentialTransportSafe(
			new URL(
				controlPlaneApiUrl("/api/credentials/manage"),
				window.location.href,
			).href,
		)
	);
}

async function request(
	command?: CredentialCommand,
	signal?: AbortSignal,
): Promise<CredentialManageResult> {
	// No query/mutation cache, error body reflection, logging, or persistence.
	const response = await authFetch(
		controlPlaneApiUrl(
			command ? "/api/credentials/manage" : "/api/credentials",
		),
		{
			method: command ? "POST" : "GET",
			cache: "no-store",
			redirect: "error",
			signal,
			...(command
				? {
						headers: { "Content-Type": "application/json" },
						body: JSON.stringify(command),
					}
				: {}),
		},
	);
	if (!response.ok)
		throw new Error(
			"Credential request failed. Refresh status before retrying.",
		);
	return response.json();
}

export function getCredentialStatus(
	signal?: AbortSignal,
): Promise<CredentialManageResult> {
	return request(undefined, signal);
}

export function manageCredentials(
	command: CredentialCommand,
	signal?: AbortSignal,
): Promise<CredentialManageResult> {
	if (
		(command.action === "initialize" ||
			command.action === "unlock" ||
			command.action === "put") &&
		!canProvisionCredentials()
	) {
		return Promise.reject(
			new Error("Credential submission requires HTTPS or loopback HTTP."),
		);
	}
	return request(command, signal);
}
