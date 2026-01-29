/**
 * @deprecated Import from @/features/admin instead
 * This file re-exports for backwards compatibility
 */
export {
	// Types
	type SessionStatus,
	type RuntimeMode,
	type AdminSession,
	type UserRole,
	type AdminUser,
	type UserStats,
	type CreateUserRequest,
	type UpdateUserRequest,
	type InviteCode,
	type InviteCodeStats,
	type CreateInviteCodeRequest,
	type BatchCreateInviteCodesRequest,
	type HostMetrics,
	type ContainerStats,
	type SessionContainerStats,
	type AdminMetricsSnapshot,
	// Query keys
	adminKeys,
	// Session hooks
	useAdminSessions,
	useForceStopSession,
	// User hooks
	useAdminUsers,
	useUserStats,
	useCreateUser,
	useUpdateUser,
	useDeleteUser,
	useActivateUser,
	useDeactivateUser,
	// Invite code hooks
	useInviteCodes,
	useInviteCodeStats,
	useCreateInviteCode,
	useCreateInviteCodesBatch,
	useRevokeInviteCode,
	useDeleteInviteCode,
	// Metrics hook
	useAdminMetrics,
} from "@/features/admin/hooks/useAdmin";
