/**
 * Dialog for managing shared workspace members.
 * Shows current members with roles, allows adding/removing members and changing roles.
 */
import { Button } from "@/components/ui/button";
import {
	Dialog,
	DialogContent,
	DialogDescription,
	DialogHeader,
	DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import {
	type MemberRole,
	type SharedWorkspaceMemberInfo,
	addMember,
	listMembers,
	removeMember,
	updateMemberRole,
} from "@/lib/api/shared-workspaces";
import { cn } from "@/lib/utils";
import {
	Crown,
	Loader2,
	Shield,
	Trash2,
	User,
	UserPlus,
	Eye,
} from "lucide-react";
import { memo, useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";

export interface SharedWorkspaceMembersDialogProps {
	open: boolean;
	onOpenChange: (open: boolean) => void;
	workspaceId: string;
	workspaceName: string;
	workspaceColor: string;
	myRole: MemberRole;
}

const ROLE_CONFIG: Record<
	MemberRole,
	{ icon: React.ComponentType<{ className?: string }>; label: string }
> = {
	owner: { icon: Crown, label: "Owner" },
	admin: { icon: Shield, label: "Admin" },
	member: { icon: User, label: "Member" },
	viewer: { icon: Eye, label: "Viewer" },
};

const ASSIGNABLE_ROLES: MemberRole[] = ["admin", "member", "viewer"];

export const SharedWorkspaceMembersDialog = memo(
	function SharedWorkspaceMembersDialog({
		open,
		onOpenChange,
		workspaceId,
		workspaceName,
		workspaceColor,
		myRole,
	}: SharedWorkspaceMembersDialogProps) {
		const { t } = useTranslation();
		const [members, setMembers] = useState<SharedWorkspaceMemberInfo[]>([]);
		const [loading, setLoading] = useState(true);
		const [addUserId, setAddUserId] = useState("");
		const [addRole, setAddRole] = useState<MemberRole>("member");
		const [addError, setAddError] = useState<string | null>(null);
		const [addSubmitting, setAddSubmitting] = useState(false);
		const canManage = myRole === "owner" || myRole === "admin";

		const fetchMembers = useCallback(async () => {
			if (!workspaceId) return;
			try {
				setLoading(true);
				const data = await listMembers(workspaceId);
				setMembers(data);
			} catch {
				// silently fail, members list just stays empty
			} finally {
				setLoading(false);
			}
		}, [workspaceId]);

		useEffect(() => {
			if (open) {
				fetchMembers();
				setAddUserId("");
				setAddError(null);
			}
		}, [open, fetchMembers]);

		const handleAddMember = useCallback(async () => {
			if (!addUserId.trim()) return;
			try {
				setAddSubmitting(true);
				setAddError(null);
				await addMember(workspaceId, {
					user_id: addUserId.trim(),
					role: addRole,
				});
				setAddUserId("");
				await fetchMembers();
			} catch (err) {
				setAddError(
					err instanceof Error ? err.message : "Failed to add member",
				);
			} finally {
				setAddSubmitting(false);
			}
		}, [workspaceId, addUserId, addRole, fetchMembers]);

		const handleRemoveMember = useCallback(
			async (userId: string) => {
				try {
					await removeMember(workspaceId, userId);
					await fetchMembers();
				} catch {
					// ignore
				}
			},
			[workspaceId, fetchMembers],
		);

		const handleRoleChange = useCallback(
			async (userId: string, newRole: MemberRole) => {
				try {
					await updateMemberRole(workspaceId, userId, { role: newRole });
					await fetchMembers();
				} catch {
					// ignore
				}
			},
			[workspaceId, fetchMembers],
		);

		return (
			<Dialog open={open} onOpenChange={onOpenChange}>
				<DialogContent className="sm:max-w-lg">
					<DialogHeader>
						<DialogTitle className="flex items-center gap-2">
							<span
								className="w-2 h-2 rounded-full"
								style={{ backgroundColor: workspaceColor }}
							/>
							{t("sharedWorkspaces.membersTitle", "Members")} -{" "}
							{workspaceName}
						</DialogTitle>
						<DialogDescription>
							{t(
								"sharedWorkspaces.membersDescription",
								"Manage who has access to this shared workspace.",
							)}
						</DialogDescription>
					</DialogHeader>

					{/* Members list */}
					<div className="space-y-1 max-h-64 overflow-y-auto">
						{loading ? (
							<div className="flex items-center justify-center py-8">
								<Loader2 className="w-5 h-5 animate-spin text-muted-foreground" />
							</div>
						) : (
							members.map((member) => {
								const roleConfig = ROLE_CONFIG[member.role];
								const RoleIcon = roleConfig.icon;
								const isOwner = member.role === "owner";

								return (
									<div
										key={member.user_id}
										className="flex items-center gap-2 px-2 py-1.5 hover:bg-sidebar-accent/50 group"
									>
										<RoleIcon
											className={cn(
												"w-3.5 h-3.5 flex-shrink-0",
												isOwner
													? "text-amber-500"
													: "text-muted-foreground",
											)}
										/>
										<span className="text-xs font-medium text-foreground flex-1 truncate">
											{member.display_name}
										</span>
										{/* Role selector */}
										{canManage && !isOwner ? (
											<select
												value={member.role}
												onChange={(e) =>
													handleRoleChange(
														member.user_id,
														e.target.value as MemberRole,
													)
												}
												className="text-[10px] bg-transparent border border-sidebar-border px-1 py-0.5 text-muted-foreground focus:outline-none focus:border-primary/50"
											>
												{ASSIGNABLE_ROLES.map((r) => (
													<option key={r} value={r}>
														{ROLE_CONFIG[r].label}
													</option>
												))}
											</select>
										) : (
											<span className="text-[10px] text-muted-foreground">
												{roleConfig.label}
											</span>
										)}
										{/* Remove button */}
										{canManage && !isOwner && (
											<button
												type="button"
												onClick={() => handleRemoveMember(member.user_id)}
												className="text-muted-foreground hover:text-destructive opacity-0 group-hover:opacity-100 transition-opacity p-1"
												title={t("common.remove", "Remove")}
											>
												<Trash2 className="w-3 h-3" />
											</button>
										)}
									</div>
								);
							})
						)}
					</div>

					{/* Add member form */}
					{canManage && (
						<div className="border-t border-sidebar-border pt-3 space-y-2">
							<div className="flex items-center gap-2">
								<UserPlus className="w-3.5 h-3.5 text-muted-foreground flex-shrink-0" />
								<Input
									value={addUserId}
									onChange={(e) => {
										setAddUserId(e.target.value);
										setAddError(null);
									}}
									placeholder={t(
										"sharedWorkspaces.addMemberPlaceholder",
										"User ID",
									)}
									className="flex-1 text-xs h-8"
									onKeyDown={(e) => {
										if (e.key === "Enter" && addUserId.trim()) handleAddMember();
									}}
								/>
								<select
									value={addRole}
									onChange={(e) => setAddRole(e.target.value as MemberRole)}
									className="text-[10px] bg-transparent border border-sidebar-border px-1 py-1 text-muted-foreground focus:outline-none focus:border-primary/50 h-8"
								>
									{ASSIGNABLE_ROLES.map((r) => (
										<option key={r} value={r}>
											{ROLE_CONFIG[r].label}
										</option>
									))}
								</select>
								<Button
									size="sm"
									onClick={handleAddMember}
									disabled={!addUserId.trim() || addSubmitting}
									className="h-8 px-3 text-xs"
								>
									{addSubmitting ? (
										<Loader2 className="w-3 h-3 animate-spin" />
									) : (
										t("common.add", "Add")
									)}
								</Button>
							</div>
							{addError && (
								<p className="text-xs text-destructive">{addError}</p>
							)}
						</div>
					)}
				</DialogContent>
			</Dialog>
		);
	},
);
