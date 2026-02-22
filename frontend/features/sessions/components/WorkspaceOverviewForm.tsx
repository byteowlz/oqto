import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Input } from "@/components/ui/input";
import {
	Select,
	SelectContent,
	SelectItem,
	SelectTrigger,
	SelectValue,
} from "@/components/ui/select";
import type { PiModelInfo } from "@/lib/api/default-chat";
import { cn } from "@/lib/utils";

export type WorkspaceOverviewValues = {
	displayName: string;
	sandboxProfile: string;
	defaultModelRef: string | null;
	skillsMode: "all" | "custom";
	extensionsMode: "all" | "custom";
	selectedSkills: string[];
	selectedExtensions: string[];
};

export type ResourceEntry = {
	name: string;
	mandatory?: boolean;
};

export interface WorkspaceOverviewFormProps {
	locale: string;
	workspacePathLabel: string;
	values: WorkspaceOverviewValues;
	availableModels: PiModelInfo[];
	sandboxProfiles: string[];
	availableSkills: string[];
	availableExtensions: ResourceEntry[];
	onChange: (values: WorkspaceOverviewValues) => void;
	onSave?: () => void;
	saving?: boolean;
	error?: string | null;
	showSave?: boolean;
}

const modeLabel = (mode: "all" | "custom", locale: string) => {
	if (mode === "all") return locale === "de" ? "Alle" : "All";
	return locale === "de" ? "Auswahl" : "Custom";
};

export function WorkspaceOverviewForm({
	locale,
	workspacePathLabel,
	values,
	availableModels,
	sandboxProfiles,
	availableSkills,
	availableExtensions,
	onChange,
	onSave,
	saving = false,
	error,
	showSave = true,
}: WorkspaceOverviewFormProps) {
	const update = (patch: Partial<WorkspaceOverviewValues>) => {
		onChange({ ...values, ...patch });
	};

	const sortedProfiles = sandboxProfiles.length
		? [...sandboxProfiles].sort()
		: ["development", "minimal", "strict"];

	const modelOptions = availableModels
		.filter((model) => model.provider && model.id)
		.map((model) => ({
			value: `${model.provider}/${model.id}`,
			label: `${model.name} (${model.provider})`,
		}));

	const mandatoryExtensions = availableExtensions.filter(
		(ext) => ext.mandatory,
	);
	const optionalExtensions = availableExtensions.filter(
		(ext) => !ext.mandatory,
	);

	const handleSkillsModeToggle = (nextMode: "all" | "custom") => {
		const selected =
			nextMode === "custom" && values.selectedSkills.length === 0
				? [...availableSkills]
				: values.selectedSkills;
		update({ skillsMode: nextMode, selectedSkills: selected });
	};

	const handleExtensionsModeToggle = (nextMode: "all" | "custom") => {
		const selected =
			nextMode === "custom" && values.selectedExtensions.length === 0
				? optionalExtensions.map((ext) => ext.name)
				: values.selectedExtensions;
		update({ extensionsMode: nextMode, selectedExtensions: selected });
	};

	const toggleSkill = (item: string) => {
		const set = new Set(values.selectedSkills);
		if (set.has(item)) {
			set.delete(item);
		} else {
			set.add(item);
		}
		update({ selectedSkills: Array.from(set).sort() });
	};

	const toggleExtension = (item: string) => {
		const set = new Set(values.selectedExtensions);
		if (set.has(item)) {
			set.delete(item);
		} else {
			set.add(item);
		}
		update({ selectedExtensions: Array.from(set).sort() });
	};

	const renderSkillsList = (
		items: string[],
		mode: "all" | "custom",
		selected: string[],
	) => (
		<div className="space-y-2">
			<div className="flex items-center gap-2">
				<Button
					variant={mode === "all" ? "default" : "outline"}
					size="sm"
					onClick={() => handleSkillsModeToggle("all")}
				>
					{modeLabel("all", locale)}
				</Button>
				<Button
					variant={mode === "custom" ? "default" : "outline"}
					size="sm"
					onClick={() => handleSkillsModeToggle("custom")}
				>
					{modeLabel("custom", locale)}
				</Button>
			</div>
			<div className="grid grid-cols-1 md:grid-cols-3 gap-2">
				{items.length === 0 && (
					<div className="text-xs text-muted-foreground">
						{locale === "de" ? "Keine Eintraege gefunden" : "No entries found"}
					</div>
				)}
				{items.map((item) => {
					const checked = mode === "all" || selected.includes(item);
					return (
						// biome-ignore lint/a11y/noLabelWithoutControl: label is associated via htmlFor
						<label
							key={item}
							className={cn(
								"flex items-center gap-2 rounded border border-border px-2 py-1 text-sm",
								mode === "all" && "opacity-60",
							)}
						>
							<Checkbox
								checked={checked}
								onCheckedChange={() => mode === "custom" && toggleSkill(item)}
								disabled={mode === "all"}
							/>
							<span className="truncate">{item}</span>
						</label>
					);
				})}
			</div>
		</div>
	);

	const renderExtensionsList = () => (
		<div className="space-y-3">
			{/* Mandatory platform extensions -- always active */}
			{mandatoryExtensions.length > 0 && (
				<div className="space-y-1.5">
					<div className="text-[11px] text-muted-foreground">
						{locale === "de"
							? "Plattform (immer aktiv)"
							: "Platform (always active)"}
					</div>
					<div className="grid grid-cols-1 md:grid-cols-3 gap-2">
						{mandatoryExtensions.map((ext) => (
							// biome-ignore lint/a11y/noLabelWithoutControl: label is associated via htmlFor
							<label
								key={ext.name}
								className="flex items-center gap-2 rounded border border-border px-2 py-1 text-sm opacity-60"
							>
								<Checkbox checked={true} disabled={true} />
								<span className="truncate">{ext.name}</span>
							</label>
						))}
					</div>
				</div>
			)}
			{/* Optional extensions -- toggleable */}
			{optionalExtensions.length > 0 && (
				<div className="space-y-1.5">
					<div className="text-[11px] text-muted-foreground">
						{locale === "de" ? "Zusaetzlich" : "Additional"}
					</div>
					<div className="flex items-center gap-2">
						<Button
							variant={values.extensionsMode === "all" ? "default" : "outline"}
							size="sm"
							onClick={() => handleExtensionsModeToggle("all")}
						>
							{modeLabel("all", locale)}
						</Button>
						<Button
							variant={
								values.extensionsMode === "custom" ? "default" : "outline"
							}
							size="sm"
							onClick={() => handleExtensionsModeToggle("custom")}
						>
							{modeLabel("custom", locale)}
						</Button>
					</div>
					<div className="grid grid-cols-1 md:grid-cols-3 gap-2">
						{optionalExtensions.map((ext) => {
							const checked =
								values.extensionsMode === "all" ||
								values.selectedExtensions.includes(ext.name);
							return (
								// biome-ignore lint/a11y/noLabelWithoutControl: label is associated via htmlFor
								<label
									key={ext.name}
									className={cn(
										"flex items-center gap-2 rounded border border-border px-2 py-1 text-sm",
										values.extensionsMode === "all" && "opacity-60",
									)}
								>
									<Checkbox
										checked={checked}
										onCheckedChange={() =>
											values.extensionsMode === "custom" &&
											toggleExtension(ext.name)
										}
										disabled={values.extensionsMode === "all"}
									/>
									<span className="truncate">{ext.name}</span>
								</label>
							);
						})}
					</div>
				</div>
			)}
		</div>
	);

	const selectedModelValue =
		values.defaultModelRef &&
		modelOptions.some((option) => option.value === values.defaultModelRef)
			? values.defaultModelRef
			: "";

	return (
		<div className="space-y-6">
			<div>
				<div className="text-xs uppercase text-muted-foreground">
					{locale === "de" ? "Arbeitsbereich" : "Workspace"}
				</div>
				<div className="text-sm font-mono text-foreground/80">
					{workspacePathLabel}
				</div>
			</div>

			<div className="space-y-2">
				<div className="text-xs uppercase text-muted-foreground">
					{locale === "de" ? "Name" : "Name"}
				</div>
				<Input
					value={values.displayName}
					onChange={(event) => update({ displayName: event.target.value })}
					placeholder={locale === "de" ? "Projektname" : "Project name"}
				/>
			</div>

			<div className="space-y-2">
				<div className="flex items-center justify-between">
					<div className="text-xs uppercase text-muted-foreground">
						{locale === "de" ? "Pi Standardmodell" : "Pi Default Model"}
					</div>
				</div>
				<p className="text-[11px] text-muted-foreground">
					{locale === "de"
						? "Standardmodell für neue Pi-Sitzungen in diesem Workspace"
						: "Default model for new Pi sessions in this workspace"}
				</p>
				<Select
					value={selectedModelValue}
					onValueChange={(value) => update({ defaultModelRef: value || null })}
				>
					<SelectTrigger>
						<SelectValue
							placeholder={
								locale === "de" ? "Modell auswählen" : "Select model"
							}
						/>
					</SelectTrigger>
					<SelectContent>
						{modelOptions.length === 0 ? (
							<div className="px-2 py-1.5 text-xs text-muted-foreground">
								{locale === "de"
									? "Keine Modelle verfügbar"
									: "No models available"}
							</div>
						) : (
							modelOptions.map((option) => (
								<SelectItem key={option.value} value={option.value}>
									{option.label}
								</SelectItem>
							))
						)}
					</SelectContent>
				</Select>
			</div>

			<div className="space-y-2">
				<div className="text-xs uppercase text-muted-foreground">
					{locale === "de" ? "Sandbox-Profil" : "Sandbox profile"}
				</div>
				<Select
					value={values.sandboxProfile}
					onValueChange={(value) => update({ sandboxProfile: value })}
				>
					<SelectTrigger>
						<SelectValue placeholder="development" />
					</SelectTrigger>
					<SelectContent>
						{sortedProfiles.map((profile) => (
							<SelectItem key={profile} value={profile}>
								{profile}
							</SelectItem>
						))}
					</SelectContent>
				</Select>
			</div>

			<div className="space-y-2">
				<div className="text-xs uppercase text-muted-foreground">
					{locale === "de" ? "Skills" : "Skills"}
				</div>
				{renderSkillsList(
					availableSkills,
					values.skillsMode,
					values.selectedSkills,
				)}
			</div>

			<div className="space-y-2">
				<div className="text-xs uppercase text-muted-foreground">
					{locale === "de" ? "Extensions" : "Extensions"}
				</div>
				{renderExtensionsList()}
			</div>

			{error && <div className="text-sm text-destructive">{error}</div>}

			{showSave && onSave && (
				<div className="flex items-center justify-end">
					<Button onClick={onSave} disabled={saving}>
						{saving
							? locale === "de"
								? "Speichern..."
								: "Saving..."
							: locale === "de"
								? "Speichern"
								: "Save changes"}
					</Button>
				</div>
			)}
		</div>
	);
}
