import { Button } from "@/components/ui/button";
import {
	Dialog,
	DialogContent,
	DialogDescription,
	DialogFooter,
	DialogHeader,
	DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import {
	type ResourceEntry,
	WorkspaceOverviewForm,
	type WorkspaceOverviewValues,
} from "@/features/sessions/components/WorkspaceOverviewForm";
import type { PiModelInfo } from "@/lib/api/default-chat";
import type { ProjectTemplateEntry } from "@/lib/control-plane-client";
import { cn } from "@/lib/utils";
import { Loader2 } from "lucide-react";
import { memo } from "react";
import { useTranslation } from "react-i18next";

export interface NewProjectDialogProps {
	open: boolean;
	onOpenChange: (open: boolean) => void;
	locale: string;
	templatesLoading: boolean;
	templatesError: string | null;
	templatesConfigured: boolean;
	projectTemplates: ProjectTemplateEntry[];
	selectedTemplatePath: string | null;
	onSelectTemplate: (path: string) => void;
	newProjectPath: string;
	onProjectPathChange: (path: string) => void;
	newProjectShared: boolean;
	onSharedChange: (shared: boolean) => void;
	newProjectError: string | null;
	newProjectSubmitting: boolean;
	newProjectSettings: WorkspaceOverviewValues;
	onProjectSettingsChange: (values: WorkspaceOverviewValues) => void;
	availableModels: PiModelInfo[];
	availableSkills: string[];
	availableExtensions: ResourceEntry[];
	sandboxProfiles: string[];
	settingsLoading: boolean;
	onSubmit: () => void;
}

export const NewProjectDialog = memo(function NewProjectDialog({
	open,
	onOpenChange,
	locale,
	templatesLoading,
	templatesError,
	templatesConfigured,
	projectTemplates,
	selectedTemplatePath,
	onSelectTemplate,
	newProjectPath,
	onProjectPathChange,
	newProjectShared,
	onSharedChange,
	newProjectError,
	newProjectSubmitting,
	newProjectSettings,
	onProjectSettingsChange,
	availableModels,
	availableSkills,
	availableExtensions,
	sandboxProfiles,
	settingsLoading,
	onSubmit,
}: NewProjectDialogProps) {
	const { t } = useTranslation();

	return (
		<Dialog open={open} onOpenChange={onOpenChange}>
			<DialogContent className="sm:max-w-xl max-h-[85vh] overflow-y-auto">
				<DialogHeader>
					<DialogTitle>
						{t("projects.newProject")}
					</DialogTitle>
					<DialogDescription>
						{t("projects.newProjectDescription")}
					</DialogDescription>
				</DialogHeader>

				<div className="space-y-4">
					<div className="space-y-2">
						<div className="text-xs uppercase text-muted-foreground">
							{t("projects.template")}
						</div>
						{templatesLoading ? (
							<div className="text-sm text-muted-foreground">
								{t("projects.loadingTemplates")}
							</div>
						) : templatesError ? (
							<div className="text-sm text-destructive">{templatesError}</div>
						) : !templatesConfigured ? (
							<div className="text-sm text-muted-foreground">
								{t("projects.templatesNotConfigured")}
							</div>
						) : projectTemplates.length === 0 ? (
							<div className="text-sm text-muted-foreground">
								{t("projects.noTemplatesFound")}
							</div>
						) : (
							<div className="grid gap-2">
								{projectTemplates.map((template) => {
									const selected = template.path === selectedTemplatePath;
									return (
										<button
											type="button"
											key={template.path}
											onClick={() => onSelectTemplate(template.path)}
											className={cn(
												"flex flex-col gap-1 border rounded px-3 py-2 text-left transition-colors",
												selected
													? "border-primary/70 bg-primary/10"
													: "border-border hover:bg-muted",
											)}
										>
											<span className="text-sm font-medium">
												{template.name}
											</span>
											{template.description && (
												<span className="text-xs text-muted-foreground">
													{template.description}
												</span>
											)}
										</button>
									);
								})}
							</div>
						)}
					</div>

					<div className="space-y-2">
						<div className="text-xs uppercase text-muted-foreground">
							{t("projects.projectPath")}
						</div>
						<Input
							value={newProjectPath}
							onChange={(e) => onProjectPathChange(e.target.value)}
							placeholder={t("projects.projectPathPlaceholder")}
						/>
						<div className="text-xs text-muted-foreground">
							{t("projects.projectPathDescription")}
						</div>
					</div>

					<div className="flex items-center justify-between border border-border rounded px-3 py-2">
						<div className="text-sm">
							{t("projects.sharedProject")}
						</div>
						<Switch
							checked={newProjectShared}
							onCheckedChange={onSharedChange}
						/>
					</div>

					<div className="space-y-2">
						<div className="text-xs uppercase text-muted-foreground">
							{t("projects.workspaceSettings")}
						</div>
						{settingsLoading ? (
							<div className="text-sm text-muted-foreground">
								{t("projects.loadingSettings")}
							</div>
						) : (
							<div className="border border-border rounded p-3">
								<WorkspaceOverviewForm
									locale={locale}
									workspacePathLabel={
										newProjectPath.trim().length > 0
											? newProjectPath.trim()
											: t("projects.newProject")
									}
									values={newProjectSettings}
									availableModels={availableModels}
									sandboxProfiles={sandboxProfiles}
									availableSkills={availableSkills}
									availableExtensions={availableExtensions}
									onChange={onProjectSettingsChange}
									showSave={false}
								/>
							</div>
						)}
					</div>

					{newProjectError && (
						<div className="text-sm text-destructive">{newProjectError}</div>
					)}
				</div>

				<DialogFooter>
					<Button
						type="button"
						variant="outline"
						onClick={() => onOpenChange(false)}
					>
						{t("common.cancel")}
					</Button>
					<Button
						type="button"
						onClick={onSubmit}
						disabled={newProjectSubmitting || templatesLoading}
					>
						{newProjectSubmitting ? (
							<>
								<Loader2 className="w-4 h-4 mr-2 animate-spin" />
								{t("projects.creating")}
							</>
						) : (
							t("projects.createProject")
						)}
					</Button>
				</DialogFooter>
			</DialogContent>
		</Dialog>
	);
});
