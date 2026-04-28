# Frontend Refactor Notes (2026-01-20)

## Overview

This refactor focuses on long-term maintainability by introducing feature-based folders, reducing cross-file coupling, and centralizing API access for Main Chat and Sessions.

## Key Changes

- Feature folders added:
  - `frontend/features/main-chat` (components, hooks, api)
  - `frontend/features/sessions` (SessionScreen, components, api)
- Audio previews added for file tree and preview surface (Main Chat + OpenCode).
- Main Chat components moved to `frontend/features/main-chat/components` and re-exported from `frontend/components/main-chat` for compatibility.
- Sessions app entry now delegates to `SessionScreen` in `frontend/features/sessions`.
- Session-specific components moved from `frontend/apps/sessions/*` into `frontend/features/sessions/components`.
- Feature API modules introduced:
  - `frontend/features/main-chat/api.ts`
  - `frontend/features/sessions/api.ts`
- Main Chat navigation handlers moved into `frontend/features/main-chat/hooks/useMainChatNavigation.ts`.

## Follow-ups

- Continue decomposing `frontend/features/sessions/SessionScreen.tsx` into smaller hooks and UI components.
- Migrate remaining imports to feature APIs as new modules grow.

## Notes

- Sidebar message search now omits the agents filter when set to "All", allowing cass to return Main Chat results.
- Sidebar message search now appends title matches for Main Chat and OpenCode sessions.
- Biome now ignores generated Tauri assets under `frontend/src-tauri/gen` to reduce lint noise.
- Oxlint now ignores generated Tauri assets via `frontend/.eslintignore`.
