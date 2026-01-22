# Changes

- 2026-01-21: Sidebar session search now filters Main Chat entries and counts Main Chat matches in results.
- 2026-01-21: Improved chat session selection stability, Main Chat sidebar refresh, and file upload handling.
- 2026-01-21: Added a rotating segment ring around stop buttons in Main Chat and OpenCode sessions.
- 2026-01-21: Refresh OpenCode chat messages on part.* SSE events to reduce missing updates.
- 2026-01-22: Fix idle session cleanup by normalizing activity timestamps in idle queries.
- 2026-01-22: Add a longer gradient-glow segment that moves along the stop button outline.
- 2026-01-22: Force fresh message fetches on event-driven refreshes to avoid stale cache.
- 2026-01-22: Remove legacy session event mapping and use WsEvent directly in chat UI.
- 2026-01-22: Normalize stop/send button padding so icon spacing matches other controls.
- 2026-01-22: Allow resuming active Main Chat sessions before a session file exists on disk.
- 2026-01-22: Drop optimistic Main Chat messages once matching server history arrives to avoid duplicates.
