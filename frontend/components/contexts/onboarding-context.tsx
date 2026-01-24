"use client";

import {
	type OnboardingState,
	type UnlockedComponents,
	type UpdateOnboardingRequest,
	activateOnboardingGodmode,
	advanceOnboardingStage,
	completeOnboarding,
	getOnboardingState,
	resetOnboarding,
	unlockOnboardingComponent,
	updateOnboardingState,
} from "@/lib/control-plane-client";
import {
	type ReactNode,
	createContext,
	useCallback,
	useContext,
	useEffect,
	useState,
} from "react";

// Re-export types for convenience
export type {
	OnboardingState,
	UnlockedComponents,
	OnboardingStage,
	UserLevel,
} from "@/lib/control-plane-client";

const defaultUnlocked: UnlockedComponents = {
	sidebar: false,
	session_list: false,
	file_tree: false,
	todo_list: false,
	canvas: false,
	memory: false,
	trx: false,
	terminal: false,
	model_picker: false,
	projects: false,
	voice: false,
	settings: false,
};

const defaultState: OnboardingState = {
	completed: false,
	stage: "language",
	language: null,
	languages: [],
	unlocked: defaultUnlocked,
	user_level: "beginner",
	godmode: false,
	started_at: null,
	completed_at: null,
	tutorial_step: 0,
	needs_onboarding: true,
};

interface OnboardingContextValue {
	state: OnboardingState;
	loading: boolean;
	error: string | null;

	// Check if a component is unlocked
	isUnlocked: (component: keyof UnlockedComponents) => boolean;

	// Actions
	refresh: () => Promise<void>;
	advanceStage: () => Promise<void>;
	unlockComponent: (component: string) => Promise<void>;
	activateGodmode: () => Promise<void>;
	complete: () => Promise<void>;
	reset: () => Promise<void>;
	updateState: (updates: UpdateOnboardingRequest) => Promise<void>;
}

const OnboardingContext = createContext<OnboardingContextValue | null>(null);

interface OnboardingProviderProps {
	children: ReactNode;
}

export function OnboardingProvider({ children }: OnboardingProviderProps) {
	const [state, setState] = useState<OnboardingState>(defaultState);
	const [loading, setLoading] = useState(true);
	const [error, setError] = useState<string | null>(null);

	const refresh = useCallback(async () => {
		try {
			setLoading(true);
			setError(null);
			const data = await getOnboardingState();
			setState(data);
		} catch (err) {
			setError(err instanceof Error ? err.message : "Unknown error");
			// On error, default to unlocked so user isn't blocked
			setState({
				...defaultState,
				completed: true,
				needs_onboarding: false,
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
			});
		} finally {
			setLoading(false);
		}
	}, []);

	const advanceStage = useCallback(async () => {
		try {
			const data = await advanceOnboardingStage();
			setState(data);
		} catch (err) {
			setError(err instanceof Error ? err.message : "Failed to advance stage");
		}
	}, []);

	const unlockComponent = useCallback(async (component: string) => {
		try {
			const data = await unlockOnboardingComponent(component);
			setState(data);
		} catch (err) {
			setError(
				err instanceof Error ? err.message : "Failed to unlock component",
			);
		}
	}, []);

	const activateGodmode = useCallback(async () => {
		try {
			const data = await activateOnboardingGodmode();
			setState(data);
		} catch (err) {
			setError(
				err instanceof Error ? err.message : "Failed to activate godmode",
			);
		}
	}, []);

	const complete = useCallback(async () => {
		try {
			const data = await completeOnboarding();
			setState(data);
		} catch (err) {
			setError(
				err instanceof Error ? err.message : "Failed to complete onboarding",
			);
		}
	}, []);

	const reset = useCallback(async () => {
		try {
			const data = await resetOnboarding();
			setState(data);
		} catch (err) {
			setError(
				err instanceof Error ? err.message : "Failed to reset onboarding",
			);
		}
	}, []);

	const updateState = useCallback(async (updates: UpdateOnboardingRequest) => {
		try {
			const data = await updateOnboardingState(updates);
			setState(data);
		} catch (err) {
			setError(
				err instanceof Error ? err.message : "Failed to update onboarding",
			);
		}
	}, []);

	const isUnlocked = useCallback(
		(component: keyof UnlockedComponents): boolean => {
			// If onboarding is complete or godmode, everything is unlocked
			if (state.completed || state.godmode) {
				return true;
			}
			return state.unlocked[component] ?? false;
		},
		[state],
	);

	// Fetch initial state
	useEffect(() => {
		refresh();
	}, [refresh]);

	const value: OnboardingContextValue = {
		state,
		loading,
		error,
		isUnlocked,
		refresh,
		advanceStage,
		unlockComponent,
		activateGodmode,
		complete,
		reset,
		updateState,
	};

	return (
		<OnboardingContext.Provider value={value}>
			{children}
		</OnboardingContext.Provider>
	);
}

/**
 * Hook to access onboarding state and actions
 */
export function useOnboarding(): OnboardingContextValue {
	const context = useContext(OnboardingContext);
	if (!context) {
		throw new Error("useOnboarding must be used within OnboardingProvider");
	}
	return context;
}

/**
 * Hook to check if a specific component is unlocked
 */
export function useIsUnlocked(component: keyof UnlockedComponents): boolean {
	const { isUnlocked } = useOnboarding();
	return isUnlocked(component);
}

/**
 * Hook to check if user needs onboarding
 */
export function useNeedsOnboarding(): boolean {
	const { state, loading } = useOnboarding();
	if (loading) return false; // Don't redirect while loading
	return state.needs_onboarding;
}
