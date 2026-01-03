import { useQuery } from "@tanstack/react-query";

// Types for models.dev API response
export interface ModelLimit {
	context: number;
	output: number;
}

export interface ModelCost {
	input: number;
	output: number;
	cache_read?: number;
	cache_write?: number;
	reasoning?: number;
}

export interface Model {
	id: string;
	name: string;
	family: string;
	attachment?: boolean;
	reasoning?: boolean;
	tool_call?: boolean;
	temperature?: boolean;
	knowledge?: string;
	release_date?: string;
	last_updated?: string;
	modalities?: {
		input: string[];
		output: string[];
	};
	open_weights?: boolean;
	cost?: ModelCost;
	limit?: ModelLimit;
}

export interface Provider {
	id: string;
	name: string;
	env: string[];
	npm?: string;
	api?: string;
	doc?: string;
	models: Record<string, Model>;
}

export type ModelsDevData = Record<string, Provider>;

// Query keys
export const modelsDevKeys = {
	all: ["models-dev"] as const,
	data: () => [...modelsDevKeys.all, "data"] as const,
};

// Fetch models.dev API data
async function fetchModelsDevData(): Promise<ModelsDevData> {
	const response = await fetch("/api/models-dev/api.json");
	if (!response.ok) {
		throw new Error(`Failed to fetch models.dev data: ${response.statusText}`);
	}
	return response.json();
}

// Hook to get all models.dev data (cached for 1 hour)
export function useModelsDevData() {
	return useQuery({
		queryKey: modelsDevKeys.data(),
		queryFn: fetchModelsDevData,
		staleTime: 60 * 60 * 1000, // 1 hour
		gcTime: 24 * 60 * 60 * 1000, // 24 hours (was cacheTime in v4)
		retry: 2,
		refetchOnWindowFocus: false,
	});
}

// Helper to get context limit for a specific provider/model
export function getContextLimit(
	data: ModelsDevData | undefined,
	providerID: string,
	modelID: string,
	defaultLimit = 200000,
): number {
	if (!data) return defaultLimit;

	const provider = data[providerID];
	if (!provider) return defaultLimit;

	const model = provider.models[modelID];
	if (!model?.limit?.context) return defaultLimit;

	return model.limit.context;
}

// Hook to get context limit for a specific model
export function useModelContextLimit(
	providerID: string | undefined,
	modelID: string | undefined,
	defaultLimit = 200000,
): number {
	const { data } = useModelsDevData();

	if (!providerID || !modelID) return defaultLimit;
	return getContextLimit(data, providerID, modelID, defaultLimit);
}
