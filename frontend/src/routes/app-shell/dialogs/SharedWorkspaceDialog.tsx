/**
 * Dialog for creating or editing a shared workspace.
 * Includes icon picker and color picker that fit the app's theme.
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
import { useResetOnOpen } from "@/hooks/use-reset-on-open";
import {
	WORKSPACE_COLORS,
	WORKSPACE_ICONS,
	type WorkspaceColor,
	type WorkspaceIconName,
} from "@/lib/api/shared-workspaces";
import { cn } from "@/lib/utils";
import { Check, Loader2 } from "lucide-react";
import { memo, useCallback, useState } from "react";
import { useTranslation } from "react-i18next";
import { WorkspaceIcon } from "../WorkspaceIcon";

export interface SharedWorkspaceDialogProps {
	open: boolean;
	onOpenChange: (open: boolean) => void;
	/** If set, we're editing an existing workspace. */
	editId?: string;
	initialName?: string;
	initialDescription?: string;
	initialIcon?: string;
	initialColor?: string;
	submitting?: boolean;
	error?: string | null;
	onSubmit: (data: {
		name: string;
		description: string;
		icon: string;
		color: string;
	}) => void;
}

export const SharedWorkspaceDialog = memo(function SharedWorkspaceDialog({
	open,
	onOpenChange,
	editId,
	initialName = "",
	initialDescription = "",
	initialIcon = "users",
	initialColor = WORKSPACE_COLORS[0],
	submitting = false,
	error = null,
	onSubmit,
}: SharedWorkspaceDialogProps) {
	const { t } = useTranslation();
	const isEdit = !!editId;

	const [name, setName] = useState(initialName);
	const [description, setDescription] = useState(initialDescription);
	const [icon, setIcon] = useState(initialIcon);
	const [color, setColor] = useState(initialColor);

	useResetOnOpen(
		open,
		() => {
			setName(initialName);
			setDescription(initialDescription);
			setIcon(initialIcon);
			setColor(initialColor);
		},
		[initialName, initialDescription, initialIcon, initialColor],
	);

	const handleSubmit = useCallback(() => {
		if (!name.trim()) return;
		onSubmit({
			name: name.trim(),
			description: description.trim(),
			icon,
			color,
		});
	}, [name, description, icon, color, onSubmit]);

	return (
		<Dialog open={open} onOpenChange={onOpenChange}>
			<DialogContent className="sm:max-w-md">
				<DialogHeader>
					<DialogTitle>
						{isEdit
							? t("sharedWorkspaces.editTitle", "Edit Shared Workspace")
							: t("sharedWorkspaces.createTitle", "New Shared Workspace")}
					</DialogTitle>
					<DialogDescription>
						{isEdit
							? t(
									"sharedWorkspaces.editDescription",
									"Update workspace settings.",
								)
							: t(
									"sharedWorkspaces.createDescription",
									"Create a collaborative workspace with shared sessions.",
								)}
					</DialogDescription>
				</DialogHeader>

				<div className="space-y-4 py-2">
					{/* Name input */}
					<div className="space-y-1">
						<label htmlFor="sw-name" className="text-xs text-muted-foreground">
							{t("common.name", "Name")}
						</label>
						<Input
							id="sw-name"
							value={name}
							onChange={(e) => setName(e.target.value)}
							placeholder={t(
								"sharedWorkspaces.namePlaceholder",
								"e.g. Frontend Team",
							)}
							autoFocus
							onKeyDown={(e) => {
								if (e.key === "Enter" && name.trim()) handleSubmit();
							}}
						/>
					</div>

					{/* Description input */}
					<div className="space-y-1">
						<label htmlFor="sw-desc" className="text-xs text-muted-foreground">
							{t("common.description", "Description")}
						</label>
						<Input
							id="sw-desc"
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
							{name || t("sharedWorkspaces.preview", "Preview")}
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
					<Button onClick={handleSubmit} disabled={!name.trim() || submitting}>
						{submitting && <Loader2 className="w-4 h-4 mr-2 animate-spin" />}
						{isEdit ? t("common.save", "Save") : t("common.create", "Create")}
					</Button>
				</DialogFooter>
			</DialogContent>
		</Dialog>
	);
});
