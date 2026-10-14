import { join } from "node:path";
import type { ModelsStore } from "@earendil-works/pi-ai";
import { type OwnerRefresh, REFRESH_WINDOW_MS } from "./owner-broker.ts";
import { ownerModelsStore } from "./owner-models.ts";

interface Runtime {
	getAuth(
		providerId: string,
		overrides: { minOAuthValidityMs: number },
	): Promise<unknown>;
}
/** Upstream classes are injected so the policy runs against the pinned code in tests too. */
export interface RefreshDeps {
	AuthStorage: { create(path: string): unknown };
	ModelRuntime: {
		create(options: {
			credentials: never;
			modelsPath: string;
			modelsStore: ModelsStore;
			allowModelNetwork: false;
			refreshOnCreate: false;
		}): Promise<Runtime>;
	};
}

export function ownerRefreshWith(deps: RefreshDeps) {
	return (owner: string): OwnerRefresh => {
		let runtime: Promise<Runtime> | undefined;
		return async (providerId) => {
			runtime ??= deps.ModelRuntime.create({
				credentials: deps.AuthStorage.create(join(owner, "auth.json")) as never,
				modelsPath: join(owner, "models.json"),
				modelsStore: ownerModelsStore(owner),
				allowModelNetwork: false,
				refreshOnCreate: false,
			});
			const result = await (await runtime).getAuth(providerId, {
				minOAuthValidityMs: REFRESH_WINDOW_MS,
			});
			if (!result)
				throw new Error(
					`Owner credential for ${providerId} could not be renewed; log in with owning Pi`,
				);
		};
	};
}
