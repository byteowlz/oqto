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
import { useResetOnOpen } from "@/hooks/use-reset-on-open";
import { authFetch, controlPlaneApiUrl, readApiError } from "@/lib/api/client";
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
	Check,
	ChevronDown,
	Crown,
	Eye,
	Loader2,
	Shield,
	Trash2,
	User,
	UserPlus,
	X,
} from "lucide-react";
import { memo, useCallback, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";

export interface SharedWorkspaceMembersDialogProps {
	open: boolean;
	onOpenChange: (open: boolean) => void;
	workspaceId: string;
	workspaceName: string;
	workspaceColor: string;
	myRole: MemberRole;
}

type UserInfo = {
	id: string;
	username: string;
	display_name: string;
	email: string;
};

const ROLE_CONFIG: Record<
	MemberRole,
	{
		icon: React.ComponentType<{ className?: string }>;
		label: string;
		description: string;
	}
> = {
	owner: {
		icon: Crown,
		label: "Owner",
		description: "Full control, can transfer ownership",
	},
	admin: {
		icon: Shield,
		label: "Admin",
		description: "Manage members and settings",
	},
	member: {
		icon: User,
		label: "Member",
		description: "Create and edit projects",
	},
	viewer: { icon: Eye, label: "Viewer", description: "Read-only access" },
};

const ASSIGNABLE_ROLES: MemberRole[] = ["admin", "member", "viewer"];

async function fetchUsers(): Promise<UserInfo[]> {
	const res = await authFetch(controlPlaneApiUrl("/api/admin/users"), {
		cache: "no-store",
		credentials: "include",
	});
	if (!res.ok) {
		// Non-admin users won't have access -- fall back to empty
		return [];
	}
	return res.json();
}

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
		const [allUsers, setAllUsers] = useState<UserInfo[]>([]);
		const [loading, setLoading] = useState(true);
		const [searchQuery, setSearchQuery] = useState("");
		const [showUserPicker, setShowUserPicker] = useState(false);
		const [addRole, setAddRole] = useState<MemberRole>("member");
		const [addError, setAddError] = useState<string | null>(null);
		const [addSubmitting, setAddSubmitting] = useState(false);
		const searchInputRef = useRef<HTMLInputElement>(null);
		const canManage = myRole === "owner" || myRole === "admin";

		const fetchMemberList = useCallback(async () => {
			if (!workspaceId) return;
			try {
				setLoading(true);
				const data = await listMembers(workspaceId);
				setMembers(data);
			} catch {
				// silently fail
			} finally {
				setLoading(false);
			}
		}, [workspaceId]);

		useResetOnOpen(
			open,
			() => {
				void fetchMemberList();
				void fetchUsers()
					.then(setAllUsers)
					.catch(() => {});
				setSearchQuery("");
				setAddError(null);
				setShowUserPicker(false);
			},
			[fetchMemberList],
		);

		const memberUserIds = useMemo(
			() => new Set(members.map((m) => m.user_id)),
			[members],
		);

		const filteredUsers = useMemo(() => {
			if (!searchQuery.trim())
				return allUsers.filter((u) => !memberUserIds.has(u.id));
			const q = searchQuery.toLowerCase();
			return allUsers.filter(
				(u) =>
					!memberUserIds.has(u.id) &&
					(u.username.toLowerCase().includes(q) ||
						u.display_name.toLowerCase().includes(q) ||
						u.email.toLowerCase().includes(q)),
			);
		}, [allUsers, memberUserIds, searchQuery]);

		const handleAddMember = useCallback(
			async (userId: string) => {
				try {
					setAddSubmitting(true);
					setAddError(null);
					await addMember(workspaceId, { user_id: userId, role: addRole });
					setSearchQuery("");
					setShowUserPicker(false);
					await fetchMemberList();
				} catch (err) {
					setAddError(
						err instanceof Error ? err.message : "Failed to add member",
					);
				} finally {
					setAddSubmitting(false);
				}
			},
			[workspaceId, addRole, fetchMemberList],
		);

		const handleRemoveMember = useCallback(
			async (userId: string) => {
				try {
					await removeMember(workspaceId, userId);
					await fetchMemberList();
				} catch {
					// ignore
				}
			},
			[workspaceId, fetchMemberList],
		);

		const handleRoleChange = useCallback(
			async (userId: string, newRole: MemberRole) => {
				try {
					await updateMemberRole(workspaceId, userId, { role: newRole });
					await fetchMemberList();
				} catch {
					// ignore
				}
			},
			[workspaceId, fetchMemberList],
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
							{workspaceName} - {t("sharedWorkspaces.membersTitle", "Members")}
						</DialogTitle>
						<DialogDescription>
							{t(
								"sharedWorkspaces.membersDescription",
								"Manage who has access to this shared workspace.",
							)}
						</DialogDescription>
					</DialogHeader>

					{/* Members list */}
					<div className="space-y-0.5 max-h-64 overflow-y-auto">
						{loading ? (
							<div className="flex items-center justify-center py-8">
								<Loader2 className="w-5 h-5 animate-spin text-muted-foreground" />
							</div>
						) : members.length === 0 ? (
							<div className="text-sm text-muted-foreground text-center py-6">
								No members yet
							</div>
						) : (
							members.map((member) => {
								const roleConfig = ROLE_CONFIG[member.role];
								const RoleIcon = roleConfig.icon;
								const isOwner = member.role === "owner";

								return (
									<div
										key={member.user_id}
										className="flex items-center gap-2 px-2 py-2 hover:bg-sidebar-accent/50 rounded group"
									>
										<RoleIcon
											className={cn(
												"w-4 h-4 flex-shrink-0",
												isOwner ? "text-amber-500" : "text-muted-foreground",
											)}
										/>
										<div className="flex-1 min-w-0">
											<div className="text-sm font-medium text-foreground truncate">
												{member.display_name}
											</div>
											<div className="text-[10px] text-muted-foreground truncate">
												{member.user_id}
											</div>
										</div>
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
												className="text-xs bg-transparent border border-sidebar-border rounded px-2 py-1 text-muted-foreground focus:outline-none focus:border-primary/50"
											>
												{ASSIGNABLE_ROLES.map((r) => (
													<option key={r} value={r}>
														{ROLE_CONFIG[r].label}
													</option>
												))}
											</select>
										) : (
											<span
												className={cn(
													"text-xs px-2 py-0.5 rounded",
													isOwner
														? "text-amber-500 bg-amber-500/10"
														: "text-muted-foreground bg-muted/50",
												)}
											>
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
												<Trash2 className="w-3.5 h-3.5" />
											</button>
										)}
									</div>
								);
							})
						)}
					</div>

					{/* Add member section */}
					{canManage && (
						<div className="border-t border-sidebar-border pt-3 space-y-2">
							<div className="text-xs uppercase text-muted-foreground font-medium">
								{t("sharedWorkspaces.addMember", "Add member")}
							</div>

							{/* Role selector for new members */}
							<div className="flex items-center gap-2">
								<span className="text-xs text-muted-foreground">
									{t("sharedWorkspaces.addAs", "Add as:")}
								</span>
								<div className="flex gap-1">
									{ASSIGNABLE_ROLES.map((r) => {
										const config = ROLE_CONFIG[r];
										const Icon = config.icon;
										const isSelected = addRole === r;
										return (
											<button
												key={r}
												type="button"
												onClick={() => setAddRole(r)}
												className={cn(
													"flex items-center gap-1 px-2 py-1 rounded text-xs transition-colors",
													isSelected
														? "bg-primary/15 text-primary border border-primary/30"
														: "text-muted-foreground hover:text-foreground hover:bg-sidebar-accent border border-transparent",
												)}
												title={config.description}
											>
												<Icon className="w-3 h-3" />
												{config.label}
											</button>
										);
									})}
								</div>
							</div>

							{/* User search */}
							<div className="relative">
								<div className="flex items-center gap-2">
									<UserPlus className="w-3.5 h-3.5 text-muted-foreground flex-shrink-0" />
									<Input
										ref={searchInputRef}
										value={searchQuery}
										onChange={(e) => {
											setSearchQuery(e.target.value);
											setShowUserPicker(true);
											setAddError(null);
										}}
										onFocus={() => setShowUserPicker(true)}
										placeholder={t(
											"sharedWorkspaces.searchUsers",
											"Search users by name or email...",
										)}
										className="flex-1 text-xs h-8"
										onKeyDown={(e) => {
											if (e.key === "Escape") {
												setShowUserPicker(false);
											}
										}}
									/>
									{searchQuery && (
										<button
											type="button"
											onClick={() => {
												setSearchQuery("");
												setShowUserPicker(false);
											}}
											className="p-1 text-muted-foreground hover:text-foreground"
										>
											<X className="w-3 h-3" />
										</button>
									)}
								</div>

								{/* User dropdown */}
								{showUserPicker && (
									<div className="absolute left-0 right-0 top-full mt-1 z-50 max-h-48 overflow-y-auto bg-popover border border-border rounded-md shadow-lg">
										{allUsers.length === 0 ? (
											<div className="px-3 py-2 text-xs text-muted-foreground">
												{t(
													"sharedWorkspaces.noUsersAvailable",
													"No users available (admin access required)",
												)}
											</div>
										) : filteredUsers.length === 0 ? (
											<div className="px-3 py-2 text-xs text-muted-foreground">
												{searchQuery
													? t(
															"sharedWorkspaces.noUsersMatch",
															"No matching users found",
														)
													: t(
															"sharedWorkspaces.allUsersAdded",
															"All users are already members",
														)}
											</div>
										) : (
											filteredUsers.map((user) => (
												<button
													key={user.id}
													type="button"
													onClick={() => handleAddMember(user.id)}
													disabled={addSubmitting}
													className="w-full flex items-center gap-2 px-3 py-2 hover:bg-sidebar-accent/50 text-left transition-colors"
												>
													<User className="w-3.5 h-3.5 text-muted-foreground flex-shrink-0" />
													<div className="flex-1 min-w-0">
														<div className="text-xs font-medium text-foreground truncate">
															{user.display_name}
														</div>
														<div className="text-[10px] text-muted-foreground truncate">
															{user.username} - {user.email}
														</div>
													</div>
													{addSubmitting ? (
														<Loader2 className="w-3 h-3 animate-spin text-muted-foreground" />
													) : (
														<Check className="w-3 h-3 text-primary opacity-0 group-hover:opacity-100" />
													)}
												</button>
											))
										)}
									</div>
								)}
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
