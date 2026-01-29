/**
 * Persona API
 * List and get persona metadata
 */

import { authFetch, controlPlaneApiUrl, readApiError } from "./client";
import type { Persona } from "./types";

/** List all available personas */
export async function listPersonas(): Promise<Persona[]> {
	const res = await authFetch(controlPlaneApiUrl("/api/personas"), {
		cache: "no-store",
		credentials: "include",
	});
	if (!res.ok) throw new Error(await readApiError(res));
	return res.json();
}

/** Get a specific persona by ID */
export async function getPersona(personaId: string): Promise<Persona> {
	const res = await authFetch(
		controlPlaneApiUrl(`/api/personas/${personaId}`),
		{
			credentials: "include",
		},
	);
	if (!res.ok) throw new Error(await readApiError(res));
	return res.json();
}
