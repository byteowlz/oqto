import type { FlashFileReader } from "./flash-hosts";

/** Host-owned browser runtime settings. Key persistence remains explicitly opt-in. */
export type FlashProviderBinding = {
	api: string;
	label: string;
	baseUrl: string;
	model: string;
	/** Discovered for this endpoint/key, never a global provider catalog. */
	models?: Array<{ id: string; api: string }>;
	apiKey?: string;
};

/** Public native metadata; numeric Durable IDs never become public Session IDs. */
export type FlashSessionDescriptor = {
	publicSessionId: string;
	workDirectoryId: string;
	name: string;
	modelId: string;
	status: "working" | "idle";
};

/** Read-only in-process projection, not a new wire protocol or history store.
 * Native message/document values are narrowed by the platform adapter.
 */
export interface FlashConversationView {
	readonly conversation: { readonly id: string | number };
	readonly entries: readonly {
		readonly id: string | number;
		readonly conversationId?: string | number;
		readonly kind?: string;
		readonly model?: readonly unknown[] | null;
	}[];
	readonly docs: {
		readonly "pi.live"?: unknown;
		readonly "pi.agent"?: unknown;
	};
}

/** The host owns construction, native persistence, transport and writer fencing.
 * Each client opens once, with one immutable public target; uncertain calls are
 * never replayed here. close releases only that client, not the shared authority.
 */
export interface FlashWorkerPort extends FlashFileReader {
	readonly filesRevision: number;
	open(
		workspace: string,
		binding: FlashProviderBinding,
		apiKey: string,
		publicSessionId?: string,
	): Promise<unknown>;
	sessions(): Promise<FlashSessionDescriptor[]>;
	createSession(
		sourcePublicId: string,
		requestId: string,
	): Promise<FlashSessionDescriptor>;
	submit(
		text: string,
		requestId: string,
		whenBusy: "steer" | "followUp" | "reject",
	): Promise<unknown>;
	selectModel(modelId: string): Promise<unknown>;
	abort(): Promise<unknown>;
	close(): Promise<void>;
}

export type FlashWorkerFactory = (
	onSnapshot: (view: FlashConversationView) => void,
) => FlashWorkerPort;
