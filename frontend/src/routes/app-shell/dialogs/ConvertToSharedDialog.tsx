/**
 * Dialog for converting a personal project into a shared workspace.
 * Shows the source path, asks for workspace name, icon, color, and members.
 */
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
import {
	WORKSPACE_COLORS,
	WORKSPACE_ICONS,
} from "@/lib/api/shared-workspaces";
import { cn } from "@/lib/utils";
import { Check, FolderInput, Loader2 } from "lucide-react";
import { memo, useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { WorkspaceIcon } from "../WorkspaceIcon";

export interface ConvertToSharedDialogProps {
	open: boolean;
	onOpenChange: (open: boolean) => void;
	sourcePath: string;
	sourceProjectName: string;
	submitting?: boolean;
	error?: string | null;
	onSubmit: (data: {
		sourcePath: string;
		name: string;
		description: string;
		icon: string;
		color: string;
	}) => void;
}

export const ConvertToSharedDialog = memo(function ConvertToSharedDialog({
	open,
	onOpenChange,
	sourcePath,
	sourceProjectName,
	submitting = false,
	error = null,
	onSubmit,
}: ConvertToSharedDialogProps) {
	const { t } = useTranslation();

	const [name, setName] = useState(sourceProjectName);
	const [description, setDescription] = useState("");
	const [icon, setIcon] = useState("code");
	const [color, setColor] = useState(WORKSPACE_COLORS[0]);

	useEffect(() => {
		if (open) {
			setName(sourceProjectName);
			setDescription("");
			setIcon("code");
			setColor(WORKSPACE_COLORS[0]);
		}
	}, [open, sourceProjectName]);

	const handleSubmit = useCallback(() => {
		if (!name.trim()) return;
		onSubmit({
			sourcePath,
			name: name.trim(),
			description: description.trim(),
			icon,
			color,
		});
	}, [sourcePath, name, description, icon, color, onSubmit]);

	return (
		<Dialog open={open} onOpenChange={onOpenChange}>
			<DialogContent className="sm:max-w-md">
				<DialogHeader>
					<DialogTitle>
						{t("sharedWorkspaces.convertTitle", "Share this Project")}
					</DialogTitle>
					<DialogDescription>
						{t(
							"sharedWorkspaces.convertDescription",
							"Create a shared workspace from this project. Files will be copied to the shared space.",
						)}
					</DialogDescription>
				</DialogHeader>

				<div className="space-y-4 py-2">
					{/* Source path indicator */}
					<div className="flex items-center gap-2 px-2 py-2 bg-muted/50 border border-border text-xs text-muted-foreground">
						<FolderInput className="w-4 h-4 flex-shrink-0" />
						<span className="font-mono truncate">{sourcePath}</span>
					</div>

					{/* Name */}
					<div className="space-y-1">
						<label
							htmlFor="convert-name"
							className="text-xs text-muted-foreground"
						>
							{t("sharedWorkspaces.workspaceName", "Workspace name")}
						</label>
						<Input
							id="convert-name"
							value={name}
							onChange={(e) => setName(e.target.value)}
							placeholder={sourceProjectName}
							autoFocus
							onKeyDown={(e) => {
								if (e.key === "Enter" && name.trim()) handleSubmit();
							}}
						/>
					</div>

					{/* Description */}
					<div className="space-y-1">
						<label
							htmlFor="convert-desc"
							className="text-xs text-muted-foreground"
						>
							{t("common.description", "Description")}
						</label>
						<Input
							id="convert-desc"
							value={description}
							onChange={(e) => setDescription(e.target.value)}
							placeholder={t(
								"sharedWorkspaces.descriptionPlaceholder",
								"Optional description",
							)}
						/>
					</div>

					{/* Icon picker */}
					<div className="space-y-1">
						<label className="text-xs text-muted-foreground">
							{t("sharedWorkspaces.icon", "Icon")}
						</label>
						<div className="grid grid-cols-8 gap-1">
							{WORKSPACE_ICONS.map((iconName) => (
								<button
									key={iconName}
									type="button"
									onClick={() => setIcon(iconName)}
									className={cn(
										"p-2 flex items-center justify-center transition-colors hover:bg-sidebar-accent",
										icon === iconName
											? "bg-sidebar-accent border border-primary/50"
											: "border border-transparent",
									)}
									title={iconName}
								>
									<WorkspaceIcon
										icon={iconName}
										color={
											icon === iconName ? color : "var(--muted-foreground)"
										}
										className="w-4 h-4"
									/>
								</button>
							))}
						</div>
					</div>

					{/* Color picker */}
					<div className="space-y-1">
						<label className="text-xs text-muted-foreground">
							{t("sharedWorkspaces.color", "Color")}
						</label>
						<div className="flex flex-wrap gap-1">
							{WORKSPACE_COLORS.map((c) => (
								<button
									key={c}
									type="button"
									onClick={() => setColor(c)}
									className={cn(
										"w-7 h-7 flex items-center justify-center transition-all",
										color === c
											? "ring-1 ring-foreground ring-offset-1 ring-offset-background"
											: "hover:scale-110",
									)}
									style={{ backgroundColor: c }}
									title={c}
								>
									{color === c && (
										<Check
											className="w-3.5 h-3.5"
											style={{
												color:
													c === WORKSPACE_COLORS[0] ? "#0f1412" : "#ffffff",
											}}
										/>
									)}
								</button>
							))}
						</div>
					</div>

					{/* Preview */}
					<div className="flex items-center gap-2 px-2 py-2 bg-sidebar-accent/50 border border-sidebar-border">
						<WorkspaceIcon icon={icon} color={color} className="w-5 h-5" />
						<span className="text-sm font-medium text-foreground">
							{name || sourceProjectName}
						</span>
					</div>

					{error && <p className="text-xs text-destructive">{error}</p>}
				</div>

				<DialogFooter>
					<Button
						variant="ghost"
						onClick={() => onOpenChange(false)}
						disabled={submitting}
					>
						{t("common.cancel", "Cancel")}
					</Button>
					<Button
						onClick={handleSubmit}
						disabled={!name.trim() || submitting}
					>
						{submitting && (
							<Loader2 className="w-4 h-4 mr-2 animate-spin" />
						)}
						{t("sharedWorkspaces.convertAction", "Share")}
					</Button>
				</DialogFooter>
			</DialogContent>
		</Dialog>
	);
});
