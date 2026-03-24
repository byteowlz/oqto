import type {
	ResourceEntry,
	WorkspaceOverviewValues,
} from "@/features/sessions/components/WorkspaceOverviewForm";
import { useMountEffect } from "@/hooks/use-mount-effect";
import type { PiModelInfo } from "@/lib/api/default-chat";
import {
	type CreateProjectFromTemplateRequest,
	type ProjectTemplateEntry,
	applyWorkspacePiResources,
	createProjectFromTemplate,
	getWorkspacePiResources,
	getWorkspaceSandbox,
	listProjectTemplates,
	listWorkspaceDirectories,
	updateSettingsValues,
	updateWorkspaceMeta,
	updateWorkspaceSandbox,
} from "@/lib/control-plane-client";
import type { ProjectLogo } from "@/lib/control-plane-client";
import { getWsManager } from "@/lib/ws-manager";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";

export interface WorkspaceDirectory {
	name: string;
	path: string;
	logo?: ProjectLogo;
}

export interface ProjectActionsState {
	// Dialog state
	newProjectDialogOpen: boolean;
	setNewProjectDialogOpen: (open: boolean) => void;
	handleNewProjectDialogChange: (open: boolean) => void;

	// Templates
	projectTemplates: ProjectTemplateEntry[];
	templatesConfigured: boolean;
	templatesLoading: boolean;
	templatesError: string | null;
	selectedTemplatePath: string | null;
	setSelectedTemplatePath: (path: string | null) => void;

	// New project form
	newProjectPath: string;
	handleNewProjectPathChange: (path: string) => void;
	newProjectShared: boolean;
	setNewProjectShared: (shared: boolean) => void;
	newProjectSubmitting: boolean;
	newProjectError: string | null;
	newProjectSettings: WorkspaceOverviewValues;
	setNewProjectSettings: (values: WorkspaceOverviewValues) => void;

	// Settings options
	availableModels: PiModelInfo[];
	availableSkills: string[];
	availableExtensions: ResourceEntry[];
	sandboxProfiles: string[];
	settingsLoading: boolean;

	// Shared workspace context (when creating a project inside a shared workspace)
	newProjectSharedWorkspaceId: string | null;

	// Actions
	handleCreateProjectFromTemplate: () => Promise<void>;
	openNewProjectForWorkspace: (
		workspacePath: string,
		sharedWorkspaceId?: string,
	) => void;

	// Workspace directories
	workspaceDirectories: WorkspaceDirectory[];
	refreshWorkspaceDirectories: () => Promise<void>;

	// Sort state
	projectSortBy: "date" | "name" | "sessions";
	setProjectSortBy: (sort: "date" | "name" | "sessions") => void;
	projectSortAsc: boolean;
	setProjectSortAsc: (asc: boolean) => void;
}

const defaultOverviewValues: WorkspaceOverviewValues = {
	displayName: "",
	sandboxProfile: "development",
	defaultModelRef: null,
	skillsMode: "all",
	extensionsMode: "all",
	selectedSkills: [],
	selectedExtensions: [],
};

export function useProjectActions(
	workspacePath?: string | null,
): ProjectActionsState {
	// Dialog states
	const [newProjectDialogOpen, setNewProjectDialogOpenRaw] = useState(false);
	const [projectTemplates, setProjectTemplates] = useState<
		ProjectTemplateEntry[]
	>([]);
	const [templatesConfigured, setTemplatesConfigured] = useState(true);
	const [templatesLoading, setTemplatesLoading] = useState(false);
	const [templatesError, setTemplatesError] = useState<string | null>(null);
	const [selectedTemplatePath, setSelectedTemplatePath] = useState<
		string | null
	>(null);
	const [newProjectPath, setNewProjectPath] = useState("");
	const [newProjectShared, setNewProjectShared] = useState(false);
	const [newProjectSharedWorkspaceId, setNewProjectSharedWorkspaceId] =
		useState<string | null>(null);
	const [newProjectSubmitting, setNewProjectSubmitting] = useState(false);
	const [newProjectError, setNewProjectError] = useState<string | null>(null);
	const [newProjectSettings, setNewProjectSettings] =
		useState<WorkspaceOverviewValues>(defaultOverviewValues);

	const [availableModels, setAvailableModels] = useState<PiModelInfo[]>([]);
	const [availableSkills, setAvailableSkills] = useState<string[]>([]);
	const [availableExtensions, setAvailableExtensions] = useState<
		ResourceEntry[]
	>([]);
	const [sandboxProfiles, setSandboxProfiles] = useState<string[]>([]);
	const [settingsLoading, setSettingsLoading] = useState(false);
	const lastTemplatePathRef = useRef<string | null>(null);

	// Project sort state
	const [projectSortBy, setProjectSortBy] = useState<
		"date" | "name" | "sessions"
	>("date");
	const [projectSortAsc, setProjectSortAsc] = useState(false);

	const [workspaceDirectories, setWorkspaceDirectories] = useState<
		WorkspaceDirectory[]
	>([]);

	const resetNewProjectForm = useCallback(() => {
		setProjectTemplates([]);
		setTemplatesLoading(false);
		setTemplatesError(null);
		setSelectedTemplatePath(null);
		setNewProjectPath("");
		setNewProjectShared(false);
		setNewProjectSharedWorkspaceId(null);
		setNewProjectSubmitting(false);
		setNewProjectError(null);
		setNewProjectSettings(defaultOverviewValues);
		lastTemplatePathRef.current = null;
	}, []);

	const templateRequestIdRef = useRef(0);

	const loadTemplates = useCallback(async () => {
		if (typeof window === "undefined") return;
		const requestId = ++templateRequestIdRef.current;
		setTemplatesLoading(true);
		setTemplatesError(null);
		try {
			const response = await listProjectTemplates();
			if (requestId !== templateRequestIdRef.current) return;
			setTemplatesConfigured(response.configured);
			setProjectTemplates(response.templates);
			if (response.templates.length > 0) {
				setSelectedTemplatePath((prev) => prev ?? response.templates[0].path);
			}
		} catch (err) {
			if (requestId !== templateRequestIdRef.current) return;
			console.error("Failed to load templates:", err);
			setTemplatesError(
				err instanceof Error ? err.message : "Failed to load templates",
			);
			setProjectTemplates([]);
		} finally {
			if (requestId === templateRequestIdRef.current) {
				setTemplatesLoading(false);
			}
		}
	}, []);

	const refreshWorkspaceDirectories = useCallback(() => {
		if (typeof window === "undefined") return Promise.resolve();
		return listWorkspaceDirectories(".")
			.then((entries) => {
				const dirs = entries.map((entry) => ({
					name: entry.name,
					path: entry.path,
					logo: entry.logo,
				}));
				setWorkspaceDirectories(dirs);
			})
			.catch((err) => {
				console.error("Failed to load workspace directories:", err);
				setWorkspaceDirectories([]);
			});
	}, []);

	const loadSettingsOptions = useCallback(async () => {
		if (!workspacePath) return;
		setSettingsLoading(true);
		try {
			const [resources, sandbox, models] = await Promise.all([
				getWorkspacePiResources(workspacePath),
				getWorkspaceSandbox(workspacePath),
				getWsManager()
					.agentGetAvailableModels("_system", workspacePath)
					.then((result) => (result as PiModelInfo[]) ?? [])
					.catch(() => [] as PiModelInfo[]),
			]);
			setAvailableSkills(resources.skills.map((skill) => skill.name));
			setAvailableExtensions(
				resources.extensions.map((ext) => ({
					name: ext.name,
					mandatory: ext.mandatory,
				})),
			);
			setSandboxProfiles(sandbox.profiles);
			setAvailableModels(models);
		} catch (err) {
			console.error("Failed to load workspace settings options:", err);
			setAvailableSkills([]);
			setAvailableExtensions([]);
			setSandboxProfiles([]);
			setAvailableModels([]);
		} finally {
			setSettingsLoading(false);
		}
	}, [workspacePath]);

	const loadDialogData = useCallback(() => {
		void Promise.all([loadSettingsOptions(), loadTemplates()]);
	}, [loadSettingsOptions, loadTemplates]);

	const handleNewProjectDialogChange = useCallback(
		(open: boolean) => {
			setNewProjectDialogOpenRaw(open);
			if (!open) {
				resetNewProjectForm();
				return;
			}
			loadDialogData();
		},
		[loadDialogData, resetNewProjectForm],
	);

	const setNewProjectDialogOpen = useCallback(
		(open: boolean) => {
			handleNewProjectDialogChange(open);
		},
		[handleNewProjectDialogChange],
	);

	/** Open the new-project dialog pre-configured for a shared workspace. */
	const openNewProjectForWorkspace = useCallback(
		(workspacePath: string, sharedWorkspaceId?: string) => {
			resetNewProjectForm();
			// For shared workspaces, path is relative (just project name).
			// For personal workspaces, pre-fill with workspace path prefix.
			if (sharedWorkspaceId) {
				setNewProjectPath("");
				setNewProjectSharedWorkspaceId(sharedWorkspaceId);
			} else {
				const prefix = workspacePath.endsWith("/")
					? workspacePath
					: `${workspacePath}/`;
				setNewProjectPath(prefix);
			}
			setNewProjectShared(true);
			handleNewProjectDialogChange(true);
		},
		[handleNewProjectDialogChange, resetNewProjectForm],
	);

	const handleNewProjectPathChange = useCallback((value: string) => {
		setNewProjectPath(value);
	}, []);

	const selectedTemplate = useMemo(
		() =>
			projectTemplates.find(
				(template) => template.path === selectedTemplatePath,
			) ?? null,
		[projectTemplates, selectedTemplatePath],
	);

	const buildSettingsFromDefaults = useCallback(
		(defaults?: ProjectTemplateEntry["defaults"] | null) => {
			const next: WorkspaceOverviewValues = {
				...defaultOverviewValues,
				selectedSkills: [...defaultOverviewValues.selectedSkills],
				selectedExtensions: [...defaultOverviewValues.selectedExtensions],
			};

			if (defaults?.display_name) {
				next.displayName = defaults.display_name ?? "";
			}
			if (defaults?.sandbox_profile) {
				next.sandboxProfile = defaults.sandbox_profile ?? "development";
			}
			if (defaults?.default_provider && defaults?.default_model) {
				next.defaultModelRef = `${defaults.default_provider}/${defaults.default_model}`;
			}
			if (defaults?.skills_mode) {
				next.skillsMode = defaults.skills_mode;
			}
			if (defaults?.extensions_mode) {
				next.extensionsMode = defaults.extensions_mode;
			}
			if (defaults?.skills && defaults.skills.length > 0) {
				next.selectedSkills = [...defaults.skills];
			}
			if (defaults?.extensions && defaults.extensions.length > 0) {
				next.selectedExtensions = [...defaults.extensions];
			}

			if (next.skillsMode === "custom" && next.selectedSkills.length === 0) {
				next.selectedSkills = [...availableSkills];
			}
			if (
				next.extensionsMode === "custom" &&
				next.selectedExtensions.length === 0
			) {
				next.selectedExtensions = availableExtensions
					.filter((ext) => !ext.mandatory)
					.map((ext) => ext.name);
			}

			if (!next.defaultModelRef && availableModels[0]) {
				next.defaultModelRef = `${availableModels[0].provider}/${availableModels[0].id}`;
			}

			return next;
		},
		[availableExtensions, availableModels, availableSkills],
	);

	// useeffect-guardrail: allow - synchronize dialog defaults when template/options change
	useEffect(() => {
		if (!newProjectDialogOpen) return;

		if (selectedTemplatePath !== lastTemplatePathRef.current) {
			const nextValues = buildSettingsFromDefaults(
				selectedTemplate?.defaults ?? null,
			);
			setNewProjectSettings(nextValues);
			lastTemplatePathRef.current = selectedTemplatePath;
			return;
		}

		setNewProjectSettings((prev) => {
			let next = prev;
			if (!prev.defaultModelRef && availableModels[0]) {
				next = {
					...next,
					defaultModelRef: `${availableModels[0].provider}/${availableModels[0].id}`,
				};
			}
			if (next.skillsMode === "custom" && next.selectedSkills.length === 0) {
				next = {
					...next,
					selectedSkills: [...availableSkills],
				};
			}
			if (
				next.extensionsMode === "custom" &&
				next.selectedExtensions.length === 0
			) {
				next = {
					...next,
					selectedExtensions: availableExtensions
						.filter((ext) => !ext.mandatory)
						.map((ext) => ext.name),
				};
			}
			return next;
		});
	}, [
		availableExtensions,
		availableModels,
		availableSkills,
		buildSettingsFromDefaults,
		newProjectDialogOpen,
		selectedTemplate?.defaults,
		selectedTemplatePath,
	]);

	const handleCreateProjectFromTemplate = useCallback(async () => {
		setNewProjectError(null);
		if (!selectedTemplatePath) {
			setNewProjectError("Select a template to continue.");
			return;
		}
		const trimmedPath = newProjectPath.trim();
		if (!trimmedPath) {
			setNewProjectError("Project directory is required.");
			return;
		}
		const payload: CreateProjectFromTemplateRequest = {
			template_path: selectedTemplatePath,
			project_path: trimmedPath,
		};
		if (newProjectShared) {
			payload.shared = true;
		}
		if (newProjectSharedWorkspaceId) {
			payload.shared_workspace_id = newProjectSharedWorkspaceId;
		}
		setNewProjectSubmitting(true);
		try {
			await createProjectFromTemplate(payload);

			// Post-creation settings only apply to personal workspaces.
			// Shared workspace projects use the shared runner's defaults.
			if (!newProjectSharedWorkspaceId) {
				const displayName = newProjectSettings.displayName.trim();
				await updateWorkspaceMeta(trimmedPath, {
					display_name: displayName.length > 0 ? displayName : null,
				});

				if (newProjectSettings.sandboxProfile) {
					await updateWorkspaceSandbox(trimmedPath, {
						profile: newProjectSettings.sandboxProfile,
					});
				}

				if (newProjectSettings.defaultModelRef) {
					const [provider, model] =
						newProjectSettings.defaultModelRef.split("/");
					if (provider && model) {
						await updateSettingsValues(
							"pi-agent",
							{
								values: {
									defaultProvider: provider,
									defaultModel: model,
								},
							},
							trimmedPath,
						);
					}
				}

				await applyWorkspacePiResources({
					workspace_path: trimmedPath,
					skills_mode: newProjectSettings.skillsMode,
					extensions_mode: newProjectSettings.extensionsMode,
					skills: newProjectSettings.selectedSkills,
					extensions: newProjectSettings.selectedExtensions,
				});
			}

			await refreshWorkspaceDirectories();
			handleNewProjectDialogChange(false);
		} catch (err) {
			setNewProjectError(
				err instanceof Error ? err.message : "Failed to create project.",
			);
		} finally {
			setNewProjectSubmitting(false);
		}
	}, [
		selectedTemplatePath,
		newProjectPath,
		newProjectShared,
		newProjectSharedWorkspaceId,
		newProjectSettings,
		refreshWorkspaceDirectories,
		handleNewProjectDialogChange,
	]);

	// Load workspace directories on mount
	useMountEffect(() => {
		void refreshWorkspaceDirectories();
	});

	return {
		newProjectDialogOpen,
		setNewProjectDialogOpen,
		handleNewProjectDialogChange,
		projectTemplates,
		templatesConfigured,
		templatesLoading,
		templatesError,
		selectedTemplatePath,
		setSelectedTemplatePath,
		newProjectPath,
		handleNewProjectPathChange,
		newProjectShared,
		setNewProjectShared,
		newProjectSharedWorkspaceId,
		newProjectSubmitting,
		newProjectError,
		newProjectSettings,
		setNewProjectSettings,
		availableModels,
		availableSkills,
		availableExtensions,
		sandboxProfiles,
		settingsLoading,
		handleCreateProjectFromTemplate,
		openNewProjectForWorkspace,
		workspaceDirectories,
		refreshWorkspaceDirectories,
		projectSortBy,
		setProjectSortBy,
		projectSortAsc,
		setProjectSortAsc,
	};
}
