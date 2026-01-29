import {
	type CreateProjectFromTemplateRequest,
	type ProjectTemplateEntry,
	createProjectFromTemplate,
	listProjectTemplates,
	listWorkspaceDirectories,
} from "@/lib/control-plane-client";
import type { ProjectLogo } from "@/lib/control-plane-client";
import { useCallback, useEffect, useState } from "react";

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

	// Actions
	handleCreateProjectFromTemplate: () => Promise<void>;

	// Workspace directories
	workspaceDirectories: WorkspaceDirectory[];
	refreshWorkspaceDirectories: () => Promise<void>;

	// Sort state
	projectSortBy: "date" | "name" | "sessions";
	setProjectSortBy: (sort: "date" | "name" | "sessions") => void;
	projectSortAsc: boolean;
	setProjectSortAsc: (asc: boolean) => void;
}

export function useProjectActions(): ProjectActionsState {
	// Dialog states
	const [newProjectDialogOpen, setNewProjectDialogOpen] = useState(false);
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
	const [newProjectSubmitting, setNewProjectSubmitting] = useState(false);
	const [newProjectError, setNewProjectError] = useState<string | null>(null);

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
		setNewProjectSubmitting(false);
		setNewProjectError(null);
	}, []);

	const handleNewProjectDialogChange = useCallback(
		(open: boolean) => {
			setNewProjectDialogOpen(open);
			if (!open) {
				resetNewProjectForm();
			}
		},
		[resetNewProjectForm],
	);

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

	const handleNewProjectPathChange = useCallback((value: string) => {
		setNewProjectPath(value);
	}, []);

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
		setNewProjectSubmitting(true);
		try {
			await createProjectFromTemplate(payload);
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
		refreshWorkspaceDirectories,
		handleNewProjectDialogChange,
	]);

	// Load workspace directories on mount
	useEffect(() => {
		refreshWorkspaceDirectories();
	}, [refreshWorkspaceDirectories]);

	// Load templates when dialog opens
	useEffect(() => {
		if (!newProjectDialogOpen || typeof window === "undefined") return;
		let active = true;
		setTemplatesLoading(true);
		setTemplatesError(null);
		listProjectTemplates()
			.then((response) => {
				if (!active) return;
				setTemplatesConfigured(response.configured);
				setProjectTemplates(response.templates);
				if (response.templates.length > 0) {
					setSelectedTemplatePath((prev) => prev ?? response.templates[0].path);
				}
			})
			.catch((err) => {
				if (!active) return;
				console.error("Failed to load templates:", err);
				setTemplatesError(
					err instanceof Error ? err.message : "Failed to load templates",
				);
				setProjectTemplates([]);
			})
			.finally(() => {
				if (active) setTemplatesLoading(false);
			});
		return () => {
			active = false;
		};
	}, [newProjectDialogOpen]);

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
		newProjectSubmitting,
		newProjectError,
		handleCreateProjectFromTemplate,
		workspaceDirectories,
		refreshWorkspaceDirectories,
		projectSortBy,
		setProjectSortBy,
		projectSortAsc,
		setProjectSortAsc,
	};
}
