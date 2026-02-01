/**
 * Control Plane Client
 *
 * This file re-exports all API functions from the modular api/ directory
 * for backwards compatibility. New code should import directly from
 * "@/lib/api" or specific submodules like "@/lib/api/auth".
 */

import * as api from "./api";

export * from "./api";
// Explicit exports to keep Vite HMR from caching missing symbols.
export const deleteWorkspacePiSession = api.deleteWorkspacePiSession;
export const searchInPiSession = api.searchInPiSession;
export const renamePiSession = api.renamePiSession;
export const getDefaultChatAssistant = api.getDefaultChatAssistant;
export const listDefaultChatPiSessions = api.listDefaultChatPiSessions;
export const listDefaultChatSessions = api.listDefaultChatSessions;
export const registerDefaultChatSession = api.registerDefaultChatSession;
export const getDefaultChatPiModels = api.getDefaultChatPiModels;
export const getDefaultChatPiState = api.getDefaultChatPiState;
export const startDefaultChatPiSession = api.startDefaultChatPiSession;
