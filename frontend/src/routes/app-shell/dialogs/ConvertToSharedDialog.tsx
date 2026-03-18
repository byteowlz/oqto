/**
 * Dialog for converting a personal project into a shared workspace.
 * Offers sharing to a new shared workspace or adding the project to an existing one.
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
	Select,
	SelectContent,
	SelectItem,
	SelectTrigger,
	SelectValue,
} from "@/components/ui/select";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { useResetOnOpen } from "@/hooks/use-reset-on-open";
import type { SharedWorkspaceInfo } from "@/lib/api/shared-workspaces";
import { WORKSPACE_COLORS, WORKSPACE_ICONS } from "@/lib/api/shared-workspaces";
import { cn } from "@/lib/utils";
import { Check, FolderInput, Loader2 } from "lucide-react";
import { memo, useCallback, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { WorkspaceIcon } from "../WorkspaceIcon";

type ShareMode = "new" | "existing";

export interface ConvertToSharedDialogProps {
	open: boolean;
	onOpenChange: (open: boolean) => void;
	sourcePath: string;
	sourceProjectName: string;
	sharedWorkspaces: SharedWorkspaceInfo[];
	submitting?: boolean;
	error?: string | null;
	onSubmit: (data: {
		sourcePath: string;
		mode: ShareMode;
		workspaceName?: string;
		description?: string;
		icon?: string;
		color?: string;
		workspaceId?: string;
		workdirName?: string;
	}) => void;
}

export const ConvertToSharedDialog = memo(function ConvertToSharedDialog({
	open,
	onOpenChange,
	sourcePath,
	sourceProjectName,
	sharedWorkspaces,
	submitting = false,
	error = null,
	onSubmit,
}: ConvertToSharedDialogProps) {
	const { t } = useTranslation();

	const eligibleWorkspaces = useMemo(
		() => sharedWorkspaces.filter((ws) => ws.my_role !== "viewer"),
		[sharedWorkspaces],
	);

	const [shareMode, setShareMode] = useState<ShareMode>("new");
	const [workspaceId, setWorkspaceId] = useState<string | undefined>(undefined);
	const [workspaceName, setWorkspaceName] = useState(sourceProjectName);
	const [workdirName, setWorkdirName] = useState(sourceProjectName);
	const [description, setDescription] = useState("");
	const [icon, setIcon] = useState("code");
	const [color, setColor] = useState(WORKSPACE_COLORS[0]);

	const selectedWorkspace = useMemo(
		() => eligibleWorkspaces.find((ws) => ws.id === workspaceId) ?? null,
		[eligibleWorkspaces, workspaceId],
	);

	useResetOnOpen(
		open,
		() => {
			setShareMode("new");
			setWorkspaceId(eligibleWorkspaces[0]?.id);
			setWorkspaceName(sourceProjectName);
			setWorkdirName(sourceProjectName);
			setDescription("");
			setIcon("code");
			setColor(WORKSPACE_COLORS[0]);
		},
		[sourceProjectName, eligibleWorkspaces],
	);

	const handleSubmit = useCallback(() => {
		if (shareMode === "new") {
			const trimmedName = workspaceName.trim();
			if (!trimmedName) return;
			onSubmit({
				sourcePath,
				mode: "new",
				workspaceName: trimmedName,
				description: description.trim(),
				icon,
				color,
			});
			return;
		}

		const trimmedWorkdir = workdirName.trim();
		if (!workspaceId || !trimmedWorkdir) return;
		onSubmit({
			sourcePath,
			mode: "existing",
			workspaceId,
			workdirName: trimmedWorkdir,
		});
	}, [
		shareMode,
		workspaceName,
		description,
		icon,
		color,
		sourcePath,
		workspaceId,
		workdirName,
		onSubmit,
	]);

	const canSubmit =
		shareMode === "new"
			? workspaceName.trim().length > 0
			: Boolean(workspaceId && workdirName.trim().length > 0);

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
							"Copy this project into a shared workspace so multiple people can collaborate.",
						)}
					</DialogDescription>
				</DialogHeader>

				<div className="space-y-4 py-2">
					{/* Source path indicator */}
					<div className="flex items-center gap-2 px-2 py-2 bg-muted/50 border border-border text-xs text-muted-foreground">
						<FolderInput className="w-4 h-4 flex-shrink-0" />
						<span className="font-mono truncate">{sourcePath}</span>
					</div>

					<Tabs
						value={shareMode}
						onValueChange={(value) => setShareMode(value as ShareMode)}
					>
						<TabsList className="w-full">
							<TabsTrigger value="new">
								{t("sharedWorkspaces.shareModeNew", "New shared workspace")}
							</TabsTrigger>
							<TabsTrigger
								value="existing"
								disabled={eligibleWorkspaces.length === 0}
							>
								{t(
									"sharedWorkspaces.shareModeExisting",
									"Existing shared workspace",
								)}
							</TabsTrigger>
						</TabsList>

						<TabsContent value="new" className="space-y-4">
							<div className="space-y-1">
								<label
									htmlFor="convert-name"
									className="text-xs text-muted-foreground"
								>
									{t("sharedWorkspaces.workspaceName", "Workspace name")}
								</label>
								<Input
									id="convert-name"
									value={workspaceName}
									onChange={(e) => setWorkspaceName(e.target.value)}
									placeholder={sourceProjectName}
									autoFocus
									onKeyDown={(e) => {
										if (e.key === "Enter" && workspaceName.trim())
											handleSubmit();
									}}
								/>
							</div>

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

							<div className="flex items-center gap-2 px-2 py-2 bg-sidebar-accent/50 border border-sidebar-border">
								<WorkspaceIcon icon={icon} color={color} className="w-5 h-5" />
								<span className="text-sm font-medium text-foreground">
									{workspaceName || sourceProjectName}
								</span>
							</div>
						</TabsContent>

						<TabsContent value="existing" className="space-y-4">
							{eligibleWorkspaces.length === 0 ? (
								<p className="text-xs text-muted-foreground">
									{t(
										"sharedWorkspaces.noExisting",
										"You do not have any shared workspaces you can write to yet.",
									)}
								</p>
							) : (
								<>
									<div className="space-y-1">
										<label className="text-xs text-muted-foreground">
											{t(
												"sharedWorkspaces.targetWorkspace",
												"Shared workspace",
											)}
										</label>
										<Select value={workspaceId} onValueChange={setWorkspaceId}>
											<SelectTrigger className="w-full">
												<SelectValue
													placeholder={t(
														"sharedWorkspaces.selectWorkspace",
														"Select a shared workspace",
													)}
												/>
											</SelectTrigger>
											<SelectContent>
												{eligibleWorkspaces.map((ws) => (
													<SelectItem key={ws.id} value={ws.id}>
														<WorkspaceIcon
															icon={ws.icon}
															color={ws.color}
															className="w-4 h-4"
														/>
														<span>{ws.name}</span>
													</SelectItem>
												))}
											</SelectContent>
										</Select>
									</div>

									<div className="space-y-1">
										<label
											htmlFor="workdir-name"
											className="text-xs text-muted-foreground"
										>
											{t("sharedWorkspaces.workdirName", "Project name")}
										</label>
										<Input
											id="workdir-name"
											value={workdirName}
											onChange={(e) => setWorkdirName(e.target.value)}
											placeholder={sourceProjectName}
											onKeyDown={(e) => {
												if (e.key === "Enter" && workdirName.trim())
													handleSubmit();
											}}
										/>
									</div>

									{selectedWorkspace && (
										<div className="flex items-center gap-2 px-2 py-2 bg-sidebar-accent/50 border border-sidebar-border">
											<WorkspaceIcon
												icon={selectedWorkspace.icon}
												color={selectedWorkspace.color}
												className="w-5 h-5"
											/>
											<span className="text-sm font-medium text-foreground">
												{selectedWorkspace.name}
											</span>
										</div>
									)}
								</>
							)}
						</TabsContent>
					</Tabs>

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
					<Button onClick={handleSubmit} disabled={!canSubmit || submitting}>
						{submitting && <Loader2 className="w-4 h-4 mr-2 animate-spin" />}
						{t("sharedWorkspaces.convertAction", "Share")}
					</Button>
				</DialogFooter>
			</DialogContent>
		</Dialog>
	);
});
