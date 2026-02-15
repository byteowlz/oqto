# Fix: Project History Vanish in Existing Chat

## Issue

When viewing an existing chat session, the session history in the sidebar would sometimes appear "vanished" or empty when a project filter was active.

## Root Cause

The issue occurred because:

1. User filters sessions by clicking on a project (e.g., "octo") in the sidebar
2. This sets `selectedProjectKey` state to filter the session list
3. User clicks on a session from the filtered list
4. The session opens in the main view, but `selectedProjectKey` is NOT cleared
5. The sidebar remains filtered to the selected project
6. This makes the session list appear incomplete or "vanished" because it only shows sessions from the filtered project

## Fix

The fix clears the `selectedProjectKey` when clicking on a session in the sidebar or when navigating to a session from search results. This ensures that when viewing a specific session, all sessions are visible in the sidebar (not just those from a filtered project).

### Changes Made

**File:** `frontend/src/routes/AppShellRoute.tsx`

1. `handleSessionClick` - Added `setSelectedProjectKey(null)` to clear the project filter when clicking on a session
2. `handleSearchResultClick` - Added `setSelectedProjectKey(null)` to clear the project filter when navigating to a session from search results

### Before

```typescript
const handleSessionClick = useCallback(
	(sessionId: string) => {
		setSelectedWorkspaceOverviewPath(null);
		setSelectedChatSessionId(sessionId);
		setActiveAppId("sessions");
		if (sessionsRoute) navigate(sessionsRoute);
		sidebarState.setMobileMenuOpen(false);
	},
	[...]
);
```

### After

```typescript
const handleSessionClick = useCallback(
	(sessionId: string) => {
		setSelectedWorkspaceOverviewPath(null);
		setSelectedChatSessionId(sessionId);
		setSelectedProjectKey(null); // Clear project filter
		setActiveAppId("sessions");
		if (sessionsRoute) navigate(sessionsRoute);
		sidebarState.setMobileMenuOpen(false);
	},
	[...]
);
```

## Testing

To verify the fix:

1. Open multiple chats across different projects/workspaces
2. Click on a project name in the sidebar to filter sessions by that project
3. Click on a session from the filtered list
4. Verify that:
   - The selected session opens in the main view
   - All sessions (not just those from the filtered project) are visible in the sidebar
   - The project filter is cleared (no "X" button showing the selected project label)

## Related Code

- `frontend/src/routes/app-shell/hooks/useSessionData.ts` - Contains the filtering logic that filters sessions by `selectedProjectKey`
- `frontend/src/routes/app-shell/SidebarSessions.tsx` - Displays the filtered session list
