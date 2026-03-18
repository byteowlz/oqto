import { useMountEffect } from "@/hooks/use-mount-effect";
import { bootstrapOnboarding } from "@/lib/control-plane-client";
import { useEffect, useState } from "react";

interface UseAppShellBootstrapInput {
	projectsLength: number;
	chatHistoryLength: number;
	currentDisplayName: string;
	locale: string;
	refreshChatHistory: () => Promise<void>;
	refreshWorkspaceSessions: () => Promise<void>;
	refreshWorkspaceDirectories: () => Promise<void>;
	setSelectedWorkspaceOverviewPath: (value: string | null) => void;
	setSelectedChatSessionId: (value: string | null) => void;
	setActiveAppId: (appId: string) => void;
	onError?: (message: string | null) => void;
}

export function useAppShellBootstrap({
	projectsLength,
	chatHistoryLength,
	currentDisplayName,
	locale,
	refreshChatHistory,
	refreshWorkspaceSessions,
	refreshWorkspaceDirectories,
	setSelectedWorkspaceOverviewPath,
	setSelectedChatSessionId,
	setActiveAppId,
	onError,
}: UseAppShellBootstrapInput): void {
	const [bootstrapReady, setBootstrapReady] = useState(false);
	const [bootstrapSubmitting, setBootstrapSubmitting] = useState(false);

	useMountEffect(() => {
		const timer = window.setTimeout(() => setBootstrapReady(true), 300);
		return () => window.clearTimeout(timer);
	});

	// useeffect-guardrail: allow - async onboarding bootstrap orchestration
	useEffect(() => {
		if (!bootstrapReady || bootstrapSubmitting) return;
		if (projectsLength > 0 || chatHistoryLength > 0) return;
		if (!currentDisplayName) return;

		setBootstrapSubmitting(true);
		onError?.(null);

		const displayName =
			currentDisplayName.charAt(0).toUpperCase() + currentDisplayName.slice(1);

		bootstrapOnboarding({
			display_name: displayName,
			language: locale,
		})
			.then(async () => {
				setSelectedWorkspaceOverviewPath(null);
				setSelectedChatSessionId(null);
				await Promise.all([
					refreshChatHistory(),
					refreshWorkspaceSessions(),
					refreshWorkspaceDirectories(),
				]);
				setActiveAppId("sessions");
			})
			.catch((error) => {
				const message =
					error instanceof Error
						? error.message
						: "Failed to bootstrap workspace";
				onError?.(message);
			})
			.finally(() => {
				setBootstrapSubmitting(false);
			});
	}, [
		bootstrapReady,
		bootstrapSubmitting,
		projectsLength,
		chatHistoryLength,
		currentDisplayName,
		locale,
		refreshChatHistory,
		refreshWorkspaceSessions,
		refreshWorkspaceDirectories,
		setSelectedWorkspaceOverviewPath,
		setSelectedChatSessionId,
		setActiveAppId,
		onError,
	]);
}
