/**
 * Onboarding API
 * User onboarding state and progression
 */

import { authFetch, controlPlaneApiUrl, readApiError } from "./client";

// ============================================================================
// Onboarding Types
// ============================================================================

export type OnboardingStage =
	| "language"
	| "provider"
	| "profile"
	| "personality"
	| "tutorial"
	| "complete";

export type UserLevel = "beginner" | "intermediate" | "technical";

export type UnlockedComponents = {
	sidebar: boolean;
	session_list: boolean;
	file_tree: boolean;
	todo_list: boolean;
	canvas: boolean;
	memory: boolean;
	trx: boolean;
	terminal: boolean;
	model_picker: boolean;
	projects: boolean;
	voice: boolean;
	settings: boolean;
};

export type OnboardingState = {
	completed: boolean;
	stage: OnboardingStage;
	language: string | null;
	languages: string[];
	unlocked: UnlockedComponents;
	user_level: UserLevel;
	godmode: boolean;
	started_at: string | null;
	completed_at: string | null;
	tutorial_step: number;
	needs_onboarding: boolean;
};

export type UpdateOnboardingRequest = {
	stage?: OnboardingStage;
	language?: string;
	languages?: string[];
	user_level?: UserLevel;
	tutorial_step?: number;
	complete?: boolean;
};

export type BootstrapOnboardingRequest = {
	display_name: string;
	language?: string;
};

export type BootstrapOnboardingResponse = {
	workspace_path: string;
};

// ============================================================================
// Onboarding API
// ============================================================================

/** Get onboarding state for the current user */
export async function getOnboardingState(): Promise<OnboardingState> {
	const res = await authFetch(controlPlaneApiUrl("/api/onboarding"), {
		credentials: "include",
	});
	if (!res.ok) {
		if (res.status === 503) {
			// Service not configured - return default completed state
			return {
				completed: true,
				stage: "complete",
				language: null,
				languages: [],
				unlocked: {
					sidebar: true,
					session_list: true,
					file_tree: true,
					todo_list: true,
					canvas: true,
					memory: true,
					trx: true,
					terminal: true,
					model_picker: true,
					projects: true,
					voice: true,
					settings: true,
				},
				user_level: "technical",
				godmode: false,
				started_at: null,
				completed_at: null,
				tutorial_step: 0,
				needs_onboarding: false,
			};
		}
		const message = await readApiError(res);
		throw new Error(message);
	}
	return res.json();
}

/** Update onboarding state */
export async function updateOnboardingState(
	updates: UpdateOnboardingRequest,
): Promise<OnboardingState> {
	const res = await authFetch(controlPlaneApiUrl("/api/onboarding"), {
		method: "PUT",
		headers: { "Content-Type": "application/json" },
		body: JSON.stringify(updates),
		credentials: "include",
	});
	if (!res.ok) {
		const message = await readApiError(res);
		throw new Error(message);
	}
	return res.json();
}

/** Advance to the next onboarding stage */
export async function advanceOnboardingStage(): Promise<OnboardingState> {
	const res = await authFetch(controlPlaneApiUrl("/api/onboarding/advance"), {
		method: "POST",
		credentials: "include",
	});
	if (!res.ok) {
		const message = await readApiError(res);
		throw new Error(message);
	}
	return res.json();
}

/** Unlock a UI component */
export async function unlockOnboardingComponent(
	component: string,
): Promise<OnboardingState> {
	const res = await authFetch(
		controlPlaneApiUrl(`/api/onboarding/unlock/${component}`),
		{
			method: "POST",
			credentials: "include",
		},
	);
	if (!res.ok) {
		const message = await readApiError(res);
		throw new Error(message);
	}
	return res.json();
}

/** Activate godmode (skip onboarding, unlock everything) */
export async function activateOnboardingGodmode(): Promise<OnboardingState> {
	const res = await authFetch(controlPlaneApiUrl("/api/onboarding/godmode"), {
		method: "POST",
		credentials: "include",
	});
	if (!res.ok) {
		const message = await readApiError(res);
		throw new Error(message);
	}
	return res.json();
}

/** Complete onboarding */
export async function completeOnboarding(): Promise<OnboardingState> {
	const res = await authFetch(controlPlaneApiUrl("/api/onboarding/complete"), {
		method: "POST",
		credentials: "include",
	});
	if (!res.ok) {
		const message = await readApiError(res);
		throw new Error(message);
	}
	return res.json();
}

/** Reset onboarding state */
export async function resetOnboarding(): Promise<OnboardingState> {
	const res = await authFetch(controlPlaneApiUrl("/api/onboarding/reset"), {
		method: "POST",
		credentials: "include",
	});
	if (!res.ok) {
		const message = await readApiError(res);
		throw new Error(message);
	}
	return res.json();
}

/** Bootstrap default workspace and initial chat session */
export async function bootstrapOnboarding(
	request: BootstrapOnboardingRequest,
): Promise<BootstrapOnboardingResponse> {
	const res = await authFetch(controlPlaneApiUrl("/api/onboarding/bootstrap"), {
		method: "POST",
		headers: { "Content-Type": "application/json" },
		body: JSON.stringify(request),
		credentials: "include",
	});
	if (!res.ok) {
		const message = await readApiError(res);
		throw new Error(message);
	}
	return res.json();
}
