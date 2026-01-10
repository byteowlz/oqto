"use client";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
	Dialog,
	DialogContent,
	DialogDescription,
	DialogFooter,
	DialogHeader,
	DialogTitle,
} from "@/components/ui/dialog";
import {
	DropdownMenu,
	DropdownMenuContent,
	DropdownMenuItem,
	DropdownMenuSeparator,
	DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
	Select,
	SelectContent,
	SelectItem,
	SelectTrigger,
	SelectValue,
} from "@/components/ui/select";
import { Skeleton } from "@/components/ui/skeleton";
import {
	type AdminUser,
	type CreateUserRequest,
	type UpdateUserRequest,
	type UserRole,
	useActivateUser,
	useAdminUsers,
	useCreateUser,
	useDeactivateUser,
	useDeleteUser,
	useUpdateUser,
	useUserStats,
} from "@/hooks/use-admin";
import {
	AlertTriangle,
	Check,
	Edit,
	MoreVertical,
	Plus,
	RefreshCw,
	Shield,
	Trash2,
	User,
	UserCheck,
	UserMinus,
	Users,
	X,
} from "lucide-react";
import { useState } from "react";

function RoleBadge({ role }: { role: UserRole }) {
	const variants: Record<UserRole, "default" | "secondary" | "outline"> = {
		admin: "default",
		user: "secondary",
		service: "outline",
	};
	const icons: Record<UserRole, React.ReactNode> = {
		admin: <Shield className="w-3 h-3" />,
		user: <User className="w-3 h-3" />,
		service: <Users className="w-3 h-3" />,
	};

	return (
		<Badge variant={variants[role]} className="text-[10px] gap-1">
			{icons[role]}
			{role}
		</Badge>
	);
}

function StatusBadge({ isActive }: { isActive: boolean }) {
	return (
		<Badge
			variant={isActive ? "default" : "outline"}
			className="text-[10px] gap-1"
		>
			{isActive ? <Check className="w-3 h-3" /> : <X className="w-3 h-3" />}
			{isActive ? "active" : "inactive"}
		</Badge>
	);
}

function formatDate(dateStr: string | null): string {
	if (!dateStr) return "Never";
	const date = new Date(dateStr);
	return date.toLocaleDateString(undefined, {
		year: "numeric",
		month: "short",
		day: "numeric",
	});
}

type CreateEditDialogProps = {
	open: boolean;
	onOpenChange: (open: boolean) => void;
	user?: AdminUser | null;
	onSave: (data: CreateUserRequest | UpdateUserRequest) => Promise<void>;
	isLoading: boolean;
};

function CreateEditDialog({
	open,
	onOpenChange,
	user,
	onSave,
	isLoading,
}: CreateEditDialogProps) {
	const isEdit = !!user;
	const [username, setUsername] = useState(user?.username ?? "");
	const [email, setEmail] = useState(user?.email ?? "");
	const [displayName, setDisplayName] = useState(user?.display_name ?? "");
	const [role, setRole] = useState<UserRole>(user?.role ?? "user");
	const [password, setPassword] = useState("");
	const [error, setError] = useState<string | null>(null);

	const handleSubmit = async (e: React.FormEvent) => {
		e.preventDefault();
		setError(null);

		try {
			if (isEdit) {
				const updateData: UpdateUserRequest = {};
				if (username !== user.username) updateData.username = username;
				if (email !== user.email) updateData.email = email;
				if (displayName !== user.display_name)
					updateData.display_name = displayName;
				if (role !== user.role) updateData.role = role;
				if (password) updateData.password = password;
				await onSave(updateData);
			} else {
				await onSave({
					username,
					email,
					display_name: displayName || undefined,
					role,
					password: password || undefined,
				});
			}
			onOpenChange(false);
		} catch (err) {
			setError(err instanceof Error ? err.message : "An error occurred");
		}
	};

	// Reset form when dialog opens with different user
	const handleOpenChange = (newOpen: boolean) => {
		if (newOpen) {
			setUsername(user?.username ?? "");
			setEmail(user?.email ?? "");
			setDisplayName(user?.display_name ?? "");
			setRole(user?.role ?? "user");
			setPassword("");
			setError(null);
		}
		onOpenChange(newOpen);
	};

	return (
		<Dialog open={open} onOpenChange={handleOpenChange}>
			<DialogContent>
				<form onSubmit={handleSubmit}>
					<DialogHeader>
						<DialogTitle>{isEdit ? "Edit User" : "Create User"}</DialogTitle>
						<DialogDescription>
							{isEdit
								? "Update the user's information below."
								: "Fill in the details to create a new user."}
						</DialogDescription>
					</DialogHeader>

					<div className="grid gap-4 py-4">
						<div className="grid gap-2">
							<Label htmlFor="username">Username</Label>
							<Input
								id="username"
								value={username}
								onChange={(e) => setUsername(e.target.value)}
								placeholder="johndoe"
								required
							/>
						</div>

						<div className="grid gap-2">
							<Label htmlFor="email">Email</Label>
							<Input
								id="email"
								type="email"
								value={email}
								onChange={(e) => setEmail(e.target.value)}
								placeholder="john@example.com"
								required
							/>
						</div>

						<div className="grid gap-2">
							<Label htmlFor="displayName">Display Name</Label>
							<Input
								id="displayName"
								value={displayName}
								onChange={(e) => setDisplayName(e.target.value)}
								placeholder="John Doe"
							/>
						</div>

						<div className="grid gap-2">
							<Label htmlFor="role">Role</Label>
							<Select
								value={role}
								onValueChange={(v) => setRole(v as UserRole)}
							>
								<SelectTrigger>
									<SelectValue />
								</SelectTrigger>
								<SelectContent>
									<SelectItem value="user">User</SelectItem>
									<SelectItem value="admin">Admin</SelectItem>
									<SelectItem value="service">Service</SelectItem>
								</SelectContent>
							</Select>
						</div>

						<div className="grid gap-2">
							<Label htmlFor="password">
								{isEdit ? "New Password (leave empty to keep)" : "Password"}
							</Label>
							<Input
								id="password"
								type="password"
								value={password}
								onChange={(e) => setPassword(e.target.value)}
								placeholder={isEdit ? "********" : "Enter password"}
							/>
						</div>

						{error && (
							<div className="text-sm text-destructive flex items-center gap-2">
								<AlertTriangle className="w-4 h-4" />
								{error}
							</div>
						)}
					</div>

					<DialogFooter>
						<Button
							type="button"
							variant="outline"
							onClick={() => onOpenChange(false)}
						>
							Cancel
						</Button>
						<Button type="submit" disabled={isLoading}>
							{isLoading && <RefreshCw className="w-4 h-4 mr-2 animate-spin" />}
							{isEdit ? "Save Changes" : "Create User"}
						</Button>
					</DialogFooter>
				</form>
			</DialogContent>
		</Dialog>
	);
}

function UserRow({
	user,
	onEdit,
	onDelete,
	onActivate,
	onDeactivate,
	isActionLoading,
}: {
	user: AdminUser;
	onEdit: () => void;
	onDelete: () => void;
	onActivate: () => void;
	onDeactivate: () => void;
	isActionLoading: boolean;
}) {
	return (
		<tr className="border-b border-border hover:bg-muted/30 transition">
			<td className="py-3 px-4">
				<div className="flex flex-col gap-1">
					<span className="text-sm text-foreground font-medium">
						{user.display_name || user.username}
					</span>
					<span className="text-xs text-muted-foreground">
						@{user.username}
					</span>
				</div>
			</td>
			<td className="py-3 px-4">
				<span className="text-sm text-muted-foreground">{user.email}</span>
			</td>
			<td className="py-3 px-4">
				<RoleBadge role={user.role} />
			</td>
			<td className="py-3 px-4">
				<StatusBadge isActive={user.is_active} />
			</td>
			<td className="py-3 px-4">
				<span className="text-xs text-muted-foreground">
					{formatDate(user.created_at)}
				</span>
			</td>
			<td className="py-3 px-4">
				<span className="text-xs text-muted-foreground">
					{formatDate(user.last_login_at)}
				</span>
			</td>
			<td className="py-3 px-4">
				<DropdownMenu>
					<DropdownMenuTrigger asChild>
						<Button
							variant="ghost"
							size="sm"
							className="h-7 w-7 p-0"
							disabled={isActionLoading}
						>
							{isActionLoading ? (
								<RefreshCw className="w-4 h-4 animate-spin" />
							) : (
								<MoreVertical className="w-4 h-4" />
							)}
						</Button>
					</DropdownMenuTrigger>
					<DropdownMenuContent align="end">
						<DropdownMenuItem onClick={onEdit}>
							<Edit className="w-4 h-4 mr-2" />
							Edit
						</DropdownMenuItem>
						{user.is_active ? (
							<DropdownMenuItem onClick={onDeactivate}>
								<UserMinus className="w-4 h-4 mr-2" />
								Deactivate
							</DropdownMenuItem>
						) : (
							<DropdownMenuItem onClick={onActivate}>
								<UserCheck className="w-4 h-4 mr-2" />
								Activate
							</DropdownMenuItem>
						)}
						<DropdownMenuSeparator />
						<DropdownMenuItem
							onClick={onDelete}
							className="text-destructive focus:text-destructive"
						>
							<Trash2 className="w-4 h-4 mr-2" />
							Delete
						</DropdownMenuItem>
					</DropdownMenuContent>
				</DropdownMenu>
			</td>
		</tr>
	);
}

function MobileUserCard({
	user,
	onEdit,
	onDelete,
	onActivate,
	onDeactivate,
	isActionLoading,
}: {
	user: AdminUser;
	onEdit: () => void;
	onDelete: () => void;
	onActivate: () => void;
	onDeactivate: () => void;
	isActionLoading: boolean;
}) {
	return (
		<div className="border border-border p-3 space-y-2">
			<div className="flex items-start justify-between gap-2">
				<div className="min-w-0">
					<p className="text-sm font-medium text-foreground truncate">
						{user.display_name || user.username}
					</p>
					<p className="text-xs text-muted-foreground">@{user.username}</p>
				</div>
				<DropdownMenu>
					<DropdownMenuTrigger asChild>
						<Button
							variant="ghost"
							size="sm"
							className="h-7 w-7 p-0 shrink-0"
							disabled={isActionLoading}
						>
							{isActionLoading ? (
								<RefreshCw className="w-4 h-4 animate-spin" />
							) : (
								<MoreVertical className="w-4 h-4" />
							)}
						</Button>
					</DropdownMenuTrigger>
					<DropdownMenuContent align="end">
						<DropdownMenuItem onClick={onEdit}>
							<Edit className="w-4 h-4 mr-2" />
							Edit
						</DropdownMenuItem>
						{user.is_active ? (
							<DropdownMenuItem onClick={onDeactivate}>
								<UserMinus className="w-4 h-4 mr-2" />
								Deactivate
							</DropdownMenuItem>
						) : (
							<DropdownMenuItem onClick={onActivate}>
								<UserCheck className="w-4 h-4 mr-2" />
								Activate
							</DropdownMenuItem>
						)}
						<DropdownMenuSeparator />
						<DropdownMenuItem
							onClick={onDelete}
							className="text-destructive focus:text-destructive"
						>
							<Trash2 className="w-4 h-4 mr-2" />
							Delete
						</DropdownMenuItem>
					</DropdownMenuContent>
				</DropdownMenu>
			</div>

			<p className="text-xs text-muted-foreground truncate">{user.email}</p>

			<div className="flex flex-wrap gap-2">
				<RoleBadge role={user.role} />
				<StatusBadge isActive={user.is_active} />
			</div>

			<div className="flex gap-4 text-xs text-muted-foreground">
				<span>Created: {formatDate(user.created_at)}</span>
				<span>Last login: {formatDate(user.last_login_at)}</span>
			</div>
		</div>
	);
}

export function UsersPanel() {
	const { data: users, isLoading, error, refetch } = useAdminUsers();
	const { data: stats } = useUserStats();
	const createUserMutation = useCreateUser();
	const updateUserMutation = useUpdateUser();
	const deleteUserMutation = useDeleteUser();
	const activateUserMutation = useActivateUser();
	const deactivateUserMutation = useDeactivateUser();

	const [dialogOpen, setDialogOpen] = useState(false);
	const [editingUser, setEditingUser] = useState<AdminUser | null>(null);
	const [actionLoadingId, setActionLoadingId] = useState<string | null>(null);

	const handleCreate = () => {
		setEditingUser(null);
		setDialogOpen(true);
	};

	const handleEdit = (user: AdminUser) => {
		setEditingUser(user);
		setDialogOpen(true);
	};

	const handleSave = async (data: CreateUserRequest | UpdateUserRequest) => {
		if (editingUser) {
			await updateUserMutation.mutateAsync({
				userId: editingUser.id,
				request: data as UpdateUserRequest,
			});
		} else {
			await createUserMutation.mutateAsync(data as CreateUserRequest);
		}
	};

	const handleDelete = async (userId: string) => {
		if (!confirm("Are you sure you want to delete this user?")) return;
		setActionLoadingId(userId);
		try {
			await deleteUserMutation.mutateAsync(userId);
		} finally {
			setActionLoadingId(null);
		}
	};

	const handleActivate = async (userId: string) => {
		setActionLoadingId(userId);
		try {
			await activateUserMutation.mutateAsync(userId);
		} finally {
			setActionLoadingId(null);
		}
	};

	const handleDeactivate = async (userId: string) => {
		setActionLoadingId(userId);
		try {
			await deactivateUserMutation.mutateAsync(userId);
		} finally {
			setActionLoadingId(null);
		}
	};

	if (error) {
		return (
			<div className="bg-card border border-border">
				<div className="border-b border-border px-3 md:px-4 py-2 md:py-3 flex items-center justify-between">
					<h2 className="text-xs md:text-sm font-semibold text-muted-foreground tracking-wider">
						USERS
					</h2>
					<Button
						variant="ghost"
						size="sm"
						onClick={() => refetch()}
						className="h-7"
					>
						<RefreshCw className="w-3 h-3" />
					</Button>
				</div>
				<div className="p-4 text-sm text-destructive flex items-center gap-2">
					<AlertTriangle className="w-4 h-4" />
					Failed to load users: {error.message}
				</div>
			</div>
		);
	}

	return (
		<>
			<div className="bg-card border border-border">
				<div className="border-b border-border px-3 md:px-4 py-2 md:py-3 flex items-center justify-between">
					<div className="flex items-center gap-3">
						<h2 className="text-xs md:text-sm font-semibold text-muted-foreground tracking-wider">
							USERS
						</h2>
						{stats && (
							<div className="flex gap-2">
								<Badge variant="secondary" className="text-[10px]">
									{stats.active} active
								</Badge>
								<Badge variant="outline" className="text-[10px]">
									{stats.admins} admins
								</Badge>
							</div>
						)}
					</div>
					<div className="flex items-center gap-2">
						<Button
							variant="ghost"
							size="sm"
							onClick={() => refetch()}
							className="h-7"
						>
							<RefreshCw className="w-3 h-3" />
						</Button>
						<Button size="sm" onClick={handleCreate} className="h-7">
							<Plus className="w-3 h-3 mr-1" />
							Add User
						</Button>
					</div>
				</div>

				{isLoading ? (
					<div className="p-4 space-y-3">
						<Skeleton className="h-12 w-full" />
						<Skeleton className="h-12 w-full" />
						<Skeleton className="h-12 w-full" />
					</div>
				) : users && users.length > 0 ? (
					<>
						{/* Mobile: Card Layout */}
						<div className="md:hidden p-3 space-y-3">
							{users.map((user) => (
								<MobileUserCard
									key={user.id}
									user={user}
									onEdit={() => handleEdit(user)}
									onDelete={() => handleDelete(user.id)}
									onActivate={() => handleActivate(user.id)}
									onDeactivate={() => handleDeactivate(user.id)}
									isActionLoading={actionLoadingId === user.id}
								/>
							))}
						</div>

						{/* Desktop: Table Layout */}
						<div className="hidden md:block p-4 overflow-x-auto">
							<table className="w-full min-w-[800px]">
								<thead>
									<tr className="border-b border-border">
										<th className="text-left py-2 px-4 text-xs font-medium text-muted-foreground tracking-wider">
											USER
										</th>
										<th className="text-left py-2 px-4 text-xs font-medium text-muted-foreground tracking-wider">
											EMAIL
										</th>
										<th className="text-left py-2 px-4 text-xs font-medium text-muted-foreground tracking-wider">
											ROLE
										</th>
										<th className="text-left py-2 px-4 text-xs font-medium text-muted-foreground tracking-wider">
											STATUS
										</th>
										<th className="text-left py-2 px-4 text-xs font-medium text-muted-foreground tracking-wider">
											CREATED
										</th>
										<th className="text-left py-2 px-4 text-xs font-medium text-muted-foreground tracking-wider">
											LAST LOGIN
										</th>
										<th className="text-left py-2 px-4 text-xs font-medium text-muted-foreground tracking-wider">
											ACTIONS
										</th>
									</tr>
								</thead>
								<tbody>
									{users.map((user) => (
										<UserRow
											key={user.id}
											user={user}
											onEdit={() => handleEdit(user)}
											onDelete={() => handleDelete(user.id)}
											onActivate={() => handleActivate(user.id)}
											onDeactivate={() => handleDeactivate(user.id)}
											isActionLoading={actionLoadingId === user.id}
										/>
									))}
								</tbody>
							</table>
						</div>
					</>
				) : (
					<div className="p-8 text-center text-sm text-muted-foreground">
						No users found
					</div>
				)}
			</div>

			<CreateEditDialog
				open={dialogOpen}
				onOpenChange={setDialogOpen}
				user={editingUser}
				onSave={handleSave}
				isLoading={createUserMutation.isPending || updateUserMutation.isPending}
			/>
		</>
	);
}
