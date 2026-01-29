/**
 * Prompts feature module.
 *
 * Provides security prompt functionality including:
 * - Real-time prompt notifications via WebSocket
 * - Prompt response handling
 * - Helper functions for prompt display
 */

// Hooks
export {
	usePrompts,
	getPromptTitle,
	getPromptIcon,
	getRemainingTime,
	type PromptSource,
	type PromptType,
	type PromptAction,
	type PromptStatus,
	type Prompt,
	type PromptMessage,
	type UsePromptsOptions,
	type UsePromptsReturn,
} from "./hooks";
