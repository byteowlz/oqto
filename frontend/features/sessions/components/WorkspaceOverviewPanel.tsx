import { Button } from "@/components/ui/button";
import {
	applyWorkspacePiResources,
	getSettingsValues,
	getWorkspaceMeta,
	getWorkspacePiResources,
	getWorkspaceSandbox,
	updateSettingsValues,
	updateWorkspaceMeta,
	updateWorkspaceSandbox,
} from "@/lib/api";
import type { PiModelInfo } from "@/lib/api/default-chat";
import { getWsManager } from "@/lib/ws-manager";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
	type ResourceEntry,
	WorkspaceOverviewForm,
	type WorkspaceOverviewValues,
} from "./WorkspaceOverviewForm";

export interface WorkspaceOverviewPanelProps {
	workspacePath: string;
	locale: string;
	onClose: () => void;
}

const emptyValues: WorkspaceOverviewValues = {
	displayName: "",
	sandboxProfile: "development",
	defaultModelRef: null,
	skillsMode: "all",
	extensionsMode: "all",
	selectedSkills: [],
	selectedExtensions: [],
};

const arrayEqual = (left: string[], right: string[]) => {
	if (left.length !== right.length) return false;
	return left.every((value, index) => value === right[index]);
};

export function WorkspaceOverviewPanel({
	workspacePath,
	locale,
	onClose,
}: WorkspaceOverviewPanelProps) {
	const [values, setValues] = useState<WorkspaceOverviewValues>(emptyValues);
	const [availableModels, setAvailableModels] = useState<PiModelInfo[]>([]);
	const [availableSkills, setAvailableSkills] = useState<string[]>([]);
	const [availableExtensions, setAvailableExtensions] = useState<
		ResourceEntry[]
	>([]);
	const [sandboxProfiles, setSandboxProfiles] = useState<string[]>([]);
	const [loading, setLoading] = useState(true);
	const [saving, setSaving] = useState(false);
	const [error, setError] = useState<string | null>(null);
	const initialValuesRef = useRef<WorkspaceOverviewValues>(emptyValues);

	const workspaceLabel = useMemo(() => workspacePath, [workspacePath]);

	const loadData = useCallback(async () => {
		setLoading(true);
		setError(null);
		try {
			const [meta, sandbox, resources, settings, models] = await Promise.all([
				getWorkspaceMeta(workspacePath),
				getWorkspaceSandbox(workspacePath),
				getWorkspacePiResources(workspacePath),
				getSettingsValues("pi-agent", workspacePath),
				getWsManager()
					.agentGetAvailableModels("_system", workspacePath)
					.then((result) => (result as PiModelInfo[]) ?? [])
					.catch(() => [] as PiModelInfo[]),
			]);

			const defaultProvider = settings.defaultProvider?.value as
				| string
				| undefined;
			const defaultModel = settings.defaultModel?.value as string | undefined;
			const modelRef =
				defaultProvider && defaultModel
					? `${defaultProvider}/${defaultModel}`
					: null;

			const skills = resources.skills.map((skill) => skill.name);
			const extensions: ResourceEntry[] = resources.extensions.map(
				(extension) => ({
					name: extension.name,
					mandatory: extension.mandatory,
				}),
			);
			const selectedSkills = resources.skills
				.filter((skill) => skill.selected)
				.map((skill) => skill.name)
				.sort();
			const selectedExtensions = resources.extensions
				.filter((extension) => extension.selected)
				.map((extension) => extension.name)
				.sort();

			const nextValues: WorkspaceOverviewValues = {
				displayName: meta.display_name ?? "",
				sandboxProfile: sandbox.profile || "development",
				defaultModelRef:
					modelRef ||
					(models[0] ? `${models[0].provider}/${models[0].id}` : null),
				skillsMode: resources.skills_mode,
				extensionsMode: resources.extensions_mode,
				selectedSkills,
				selectedExtensions,
			};

			initialValuesRef.current = nextValues;
			setValues(nextValues);
			setAvailableModels(models);
			setAvailableSkills(skills);
			setAvailableExtensions(extensions);
			setSandboxProfiles(sandbox.profiles);
		} catch (err) {
			setError(err instanceof Error ? err.message : "Failed to load workspace");
		} finally {
			setLoading(false);
		}
	}, [workspacePath]);

	useEffect(() => {
		void loadData();
	}, [loadData]);

	const handleSave = useCallback(async () => {
		setSaving(true);
		setError(null);
		const initial = initialValuesRef.current;
		try {
			if (values.displayName !== initial.displayName) {
				await updateWorkspaceMeta(workspacePath, {
					display_name: values.displayName.trim() || null,
				});
			}

			if (values.sandboxProfile !== initial.sandboxProfile) {
				await updateWorkspaceSandbox(workspacePath, {
					profile: values.sandboxProfile,
				});
			}

			if (
				values.defaultModelRef &&
				values.defaultModelRef !== initial.defaultModelRef
			) {
				const [provider, model] = values.defaultModelRef.split("/");
				if (provider && model) {
					await updateSettingsValues(
						"pi-agent",
						{
							values: {
								defaultProvider: provider,
								defaultModel: model,
							},
						},
						workspacePath,
					);
				}
			}

			const skillsChanged =
				values.skillsMode !== initial.skillsMode ||
				!arrayEqual(values.selectedSkills, initial.selectedSkills);
			const extensionsChanged =
				values.extensionsMode !== initial.extensionsMode ||
				!arrayEqual(values.selectedExtensions, initial.selectedExtensions);

			if (skillsChanged || extensionsChanged) {
				await applyWorkspacePiResources({
					workspace_path: workspacePath,
					skills_mode: values.skillsMode,
					extensions_mode: values.extensionsMode,
					skills: values.selectedSkills,
					extensions: values.selectedExtensions,
				});
			}

			initialValuesRef.current = values;
		} catch (err) {
			setError(err instanceof Error ? err.message : "Failed to save settings");
		} finally {
			setSaving(false);
		}
	}, [values, workspacePath]);

	return (
		<div className="h-full flex flex-col overflow-hidden">
			<div className="flex-shrink-0 flex items-center justify-between border-b border-border pb-3 mb-4">
				<div>
					<div className="text-base font-semibold">
						{locale === "de" ? "Workspace Overview" : "Workspace overview"}
					</div>
					<div className="text-xs text-muted-foreground">
						{locale === "de"
							? "Projektweite Einstellungen fur Pi"
							: "Project-wide settings for Pi agent"}
					</div>
				</div>
				<Button variant="outline" size="sm" onClick={onClose}>
					{locale === "de" ? "Zuruck" : "Back"}
				</Button>
			</div>

			{loading ? (
				<div className="text-sm text-muted-foreground">
					{locale === "de" ? "Lade..." : "Loading..."}
				</div>
			) : (
				<div className="flex-1 min-h-0 overflow-y-auto">
					<WorkspaceOverviewForm
						locale={locale}
						workspacePathLabel={workspaceLabel}
						values={values}
						availableModels={availableModels}
						sandboxProfiles={sandboxProfiles}
						availableSkills={availableSkills}
						availableExtensions={availableExtensions}
						onChange={setValues}
						onSave={handleSave}
						saving={saving}
						error={error}
					/>
				</div>
			)}
		</div>
	);
}
