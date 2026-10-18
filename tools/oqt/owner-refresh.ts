// Private-runtime only: renew an owner credential with upstream Pi's own
// file-locked AuthStorage and provider refresh code, exactly as owning Pi
// would. Only that provider's entry is rewritten under the owner auth lock.
import { AuthStorage } from "../core/auth-storage.ts";
import { ModelRuntime } from "../core/model-runtime.ts";
import { ownerRefreshWith } from "./owner-refresh-core.ts";

export const ownerRefresh = ownerRefreshWith({ AuthStorage, ModelRuntime });
