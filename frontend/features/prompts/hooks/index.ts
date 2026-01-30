/**
 * Prompts feature hooks.
 *
 * This module exports all hooks related to the security prompt system:
 * - Prompt subscription via WebSocket
 * - Prompt response handling
 */

export {
	usePrompts,
	getPromptTitle,
	getPromptIcon,
	getRemainingTime,
} from "./usePrompts";
export type {
	PromptSource,
	PromptType,
	PromptAction,
	PromptStatus,
	Prompt,
	PromptMessage,
	UsePromptsOptions,
	UsePromptsReturn,
} from "./usePrompts";
