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
import type { ProjectTemplateEntry } from "@/lib/control-plane-client";
import { cn } from "@/lib/utils";
import { Loader2 } from "lucide-react";
import { memo } from "react";

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
	onSubmit,
}: NewProjectDialogProps) {
	return (
		<Dialog open={open} onOpenChange={onOpenChange}>
			<DialogContent className="sm:max-w-xl">
				<DialogHeader>
					<DialogTitle>
						{locale === "de" ? "Neues Projekt" : "New project"}
					</DialogTitle>
					<DialogDescription>
						{locale === "de"
							? "Ein Template auswahlen und ein neues Projekt anlegen."
							: "Pick a template and create a new project."}
					</DialogDescription>
				</DialogHeader>

				<div className="space-y-4">
					<div className="space-y-2">
						<div className="text-xs uppercase text-muted-foreground">
							{locale === "de" ? "Template" : "Template"}
						</div>
						{templatesLoading ? (
							<div className="text-sm text-muted-foreground">
								{locale === "de" ? "Lade Templates..." : "Loading templates..."}
							</div>
						) : templatesError ? (
							<div className="text-sm text-destructive">{templatesError}</div>
						) : !templatesConfigured ? (
							<div className="text-sm text-muted-foreground">
								{locale === "de"
									? "Templates nicht konfiguriert. Setze [templates].repo_path in config.toml."
									: "Templates not configured. Set [templates].repo_path in config.toml."}
							</div>
						) : projectTemplates.length === 0 ? (
							<div className="text-sm text-muted-foreground">
								{locale === "de"
									? "Keine Templates gefunden."
									: "No templates found."}
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
							{locale === "de" ? "Projektpfad" : "Project path"}
						</div>
						<Input
							value={newProjectPath}
							onChange={(e) => onProjectPathChange(e.target.value)}
							placeholder={
								locale === "de" ? "z.B. client-app" : "e.g. client-app"
							}
						/>
						<div className="text-xs text-muted-foreground">
							{locale === "de"
								? "Relativ zum Workspace-Ordner."
								: "Relative to the workspace root."}
						</div>
					</div>

					<div className="flex items-center justify-between border border-border rounded px-3 py-2">
						<div className="text-sm">
							{locale === "de" ? "Geteiltes Projekt" : "Shared project"}
						</div>
						<Switch
							checked={newProjectShared}
							onCheckedChange={onSharedChange}
						/>
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
						{locale === "de" ? "Abbrechen" : "Cancel"}
					</Button>
					<Button
						type="button"
						onClick={onSubmit}
						disabled={newProjectSubmitting || templatesLoading}
					>
						{newProjectSubmitting ? (
							<>
								<Loader2 className="w-4 h-4 mr-2 animate-spin" />
								{locale === "de" ? "Erstelle..." : "Creating..."}
							</>
						) : locale === "de" ? (
							"Projekt erstellen"
						) : (
							"Create project"
						)}
					</Button>
				</DialogFooter>
			</DialogContent>
		</Dialog>
	);
});
