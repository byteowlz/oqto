/**
 * The Action port (ADR-0045). Oqto owns its resource menus; Apps and scripts
 * only advertise eligibility, and the Action Broker stays the single
 * authority for matching, authorization and invocation. A surface asks what
 * is eligible for a set of typed subjects and asks the Broker to run one; it
 * never becomes a second execution authority.
 */

/** Where the host is projecting Actions. */
export type ActionSurface = "resource-menu" | "command-palette";

/**
 * What an Action acts on. Matching uses typed subjects, never host paths:
 * `reference` is opaque to the App and only the host resolves it.
 */
export interface ResourceSubject {
	readonly kind: "workspace-file";
	/** Binding identity: which work directory the reference belongs to. */
	readonly workspacePath: string;
	readonly reference: string;
	readonly mediaType: string;
	/** Bounded label for presentation; never used for matching. */
	readonly label: string;
}

/** One eligible Action, already matched by the Broker for this surface. */
export interface ActionOffer {
	readonly id: string;
	readonly title: string;
	/** The App that contributed it, when it came from one. */
	readonly appId?: string;
}

export interface ActionHost {
	/** Contributed Actions eligible for these subjects on this surface. */
	offers(
		surface: ActionSurface,
		subjects: readonly ResourceSubject[],
	): Promise<readonly ActionOffer[]>;
	/** Asks the Broker to run one; the surface never executes anything itself. */
	invoke(actionId: string, subjects: readonly ResourceSubject[]): Promise<void>;
}

/**
 * No Action Broker exists yet, so nothing is contributed. The seam is here so
 * the menu is a projection from the start rather than a hardcoded list that
 * would have to be rewritten when Apps and scripts can contribute.
 */
export function createEmptyActionHost(): ActionHost {
	return {
		async offers() {
			return [];
		},
		async invoke() {},
	};
}
