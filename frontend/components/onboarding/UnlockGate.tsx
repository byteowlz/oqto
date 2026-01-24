import type { UnlockedComponents } from "@/components/contexts/onboarding-context";
import { useOnboarding } from "@/components/contexts/onboarding-context";
import type { ReactNode } from "react";

interface UnlockGateProps {
	/** Component key to check */
	component: keyof UnlockedComponents;
	/** Content to show when unlocked */
	children: ReactNode;
	/** Optional content to show when locked (default: nothing) */
	fallback?: ReactNode;
	/** If true, show a subtle placeholder instead of nothing */
	showPlaceholder?: boolean;
}

/**
 * Conditionally renders children based on whether a component is unlocked.
 * 
 * Usage:
 * ```tsx
 * <UnlockGate component="terminal">
 *   <TerminalPanel />
 * </UnlockGate>
 * ```
 */
export function UnlockGate({
	component,
	children,
	fallback,
	showPlaceholder = false,
}: UnlockGateProps) {
	const { isUnlocked, loading } = useOnboarding();

	// While loading, show nothing to avoid flash
	if (loading) {
		return null;
	}

	// If unlocked, show the content
	if (isUnlocked(component)) {
		return <>{children}</>;
	}

	// If locked, show fallback or placeholder
	if (fallback) {
		return <>{fallback}</>;
	}

	if (showPlaceholder) {
		return (
			<div 
				className="opacity-20 pointer-events-none select-none"
				aria-hidden="true"
			>
				{children}
			</div>
		);
	}

	return null;
}

/**
 * Hook variant for more complex conditional logic
 */
export function useUnlockGate(component: keyof UnlockedComponents) {
	const { isUnlocked, loading, unlockComponent } = useOnboarding();
	
	return {
		isUnlocked: isUnlocked(component),
		loading,
		unlock: () => unlockComponent(component),
	};
}
