/** The oqt TUI's local plugin API (ADR-0038/0071). Plugins are bundled,
 * first-party modules loaded from the verified runtime, never from a host.
 * The shapes match Pi's retired experimental plugin API so the permission and
 * model UI ports unchanged. Everything here is process-local presentation:
 * host authority stays in the guard and the server facade.
 */
import type { Context, Ref } from "@earendil-works/chord";
import type { ConversationView } from "@earendil-works/pi-durable";
import type { Conversation, Session, Sessions } from "./contract.ts";

export interface PresentationSelectItem {
	readonly value: string;
	readonly label: string;
	readonly description?: string;
}

/** Narrow process-local UI capabilities for plugins. */
export interface PresentationUI {
	select(
		title: string,
		items: readonly PresentationSelectItem[],
		selectedValue: string | undefined,
		context: Context,
	): Promise<string | undefined>;
	showStatus(message: string, context: Context): void;
}

export interface SlashCommandCompletion {
	readonly value: string;
	readonly label: string;
	readonly description?: string;
}

/** What a command reports: an accepted id, a refusal the user may see, or nothing. */
export type SlashCommandRunResult =
	| { accepted: true; id: string }
	| { accepted: false; error: string }
	| undefined;

export interface SlashCommandContribution {
	readonly name: string;
	readonly description?: string;
	readonly argumentHint?: string;
	getArgumentCompletions?(
		argumentPrefix: string,
	):
		| readonly SlashCommandCompletion[]
		| null
		| Promise<readonly SlashCommandCompletion[] | null>;
	run(
		args: string,
		context: Context,
	): SlashCommandRunResult | Promise<SlashCommandRunResult>;
}

export interface SlashCommands {
	register(command: SlashCommandContribution): () => void;
	/** Same-name replacement; the first registration stays visible until it is released. */
	replace(command: SlashCommandContribution): () => void;
	list(): readonly SlashCommandContribution[];
	subscribe(
		listener: (commands: readonly SlashCommandContribution[]) => void,
	): () => void;
}

interface RegisteredSlashCommand {
	readonly command: SlashCommandContribution;
	closed: boolean;
}

/** First registration wins (oqt's reviewed plugin registers first). */
export class SlashCommandRegistry implements SlashCommands {
	readonly #commands = new Map<string, RegisteredSlashCommand[]>();
	readonly #listeners = new Set<
		(commands: readonly SlashCommandContribution[]) => void
	>();

	register(command: SlashCommandContribution): () => void {
		this.#validate(command);
		if (this.#commands.has(command.name))
			throw new Error(`Slash command /${command.name} is already registered`);
		return this.#add(command);
	}

	replace(command: SlashCommandContribution): () => void {
		this.#validate(command);
		return this.#add(command);
	}

	list(): readonly SlashCommandContribution[] {
		return Object.freeze(
			[...this.#commands.values()]
				.map((entries) => entries[0]?.command)
				.filter(
					(command): command is SlashCommandContribution =>
						command !== undefined,
				),
		);
	}

	subscribe(
		listener: (commands: readonly SlashCommandContribution[]) => void,
	): () => void {
		this.#listeners.add(listener);
		listener(this.list());
		return () => this.#listeners.delete(listener);
	}

	#add(command: SlashCommandContribution): () => void {
		const entry: RegisteredSlashCommand = {
			command: Object.freeze({ ...command }),
			closed: false,
		};
		const entries = this.#commands.get(command.name) ?? [];
		entries.push(entry);
		this.#commands.set(command.name, entries);
		if (entries.length === 1) this.#publish();
		return () => {
			if (entry.closed) return;
			entry.closed = true;
			if (entries[0] !== entry) return;
			while (entries[0]?.closed) entries.shift();
			if (entries.length === 0) this.#commands.delete(command.name);
			this.#publish();
		};
	}

	#validate(command: SlashCommandContribution): void {
		if (!/^[a-z0-9][a-z0-9:-]*$/u.test(command.name))
			throw new TypeError(`Invalid slash command name: ${command.name}`);
	}

	#publish(): void {
		const commands = this.list();
		for (const listener of this.#listeners) listener(commands);
	}
}

/** Semantic display only, never an execution/action API. Untrusted strings are
 * escaped before theme tokens apply; raw arguments/results stay behind the
 * component's standard expansion. */
export interface ToolPresentationInput {
	readonly args: unknown;
	readonly phase: "preparing" | "pending" | "complete";
	readonly result?: { readonly isError: boolean; readonly details?: unknown };
}
export interface ToolPresentationSummary {
	readonly title: string;
	readonly lines: readonly string[];
}
export type ToolPresentationRenderer = (
	input: ToolPresentationInput,
) => ToolPresentationSummary;
export interface ToolPresentation {
	register(
		name: "request_access",
		renderer: ToolPresentationRenderer,
	): () => void;
	render(
		name: string,
		input: ToolPresentationInput,
	): ToolPresentationSummary | undefined;
}

/** No plugin can smuggle terminal escapes, CR, bidi overrides or hidden controls.
 * Escape rather than strip so paths remain distinguishable for operator review. */
export function escapeToolPresentation(value: string): string {
	return Array.from(value, (character) => {
		const code = character.codePointAt(0) ?? 0;
		return code < 32 ||
			(code >= 127 && code <= 159) ||
			/[\p{Cf}\p{Zl}\p{Zp}]/u.test(character)
			? `\\u${code.toString(16).padStart(4, "0")}`
			: character;
	}).join("");
}

export function createToolPresentation(): ToolPresentation {
	const renderers = new Map<string, ToolPresentationRenderer>();
	return {
		register(name, renderer) {
			if (name !== "request_access")
				throw new Error("Unsupported semantic tool presentation");
			renderers.set(name, renderer);
			return () => {
				if (renderers.get(name) === renderer) renderers.delete(name);
			};
		},
		render(name, input) {
			return renderers.get(name)?.(input);
		},
	};
}

export type SessionRef = Ref<typeof Session>;
export type ParticipantRef = Extract<
	Ref<typeof Conversation>,
	{ variant: "participant" }
>;
export type SessionsRef = Ref<typeof Sessions>;

/** What the TUI shows right now: one Session, its selected conversation and view. */
export interface TuiCurrent {
	readonly session: string;
	readonly sessionRef: SessionRef;
	readonly conversation: ParticipantRef;
	readonly view: ConversationView;
}

/** Everything a bundled plugin may use. */
export interface TuiPluginApi {
	readonly ui: PresentationUI;
	readonly commands: SlashCommands;
	readonly tools: ToolPresentation;
	/** The attached Session and conversation, or undefined while (re)attaching. */
	current(): TuiCurrent | undefined;
	/** Called after every change of the attachment, selection or view. */
	onChange(listener: () => void): () => void;
	sessions(): SessionsRef | undefined;
	switchSession(session: string): Promise<void>;
	newSession(): Promise<void>;
}

/** A bundled plugin: installs with the API, returns its cleanup. */
export type TuiPlugin = (api: TuiPluginApi) => () => void;
