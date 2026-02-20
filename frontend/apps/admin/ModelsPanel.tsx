"use client";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
	type EavsProviderSummary,
	type SyncAllModelsResponse,
	useDeleteEavsProvider,
	useEavsProviders,
	useSyncAllModels,
	useUpsertEavsProvider,
} from "@/hooks/use-admin";
import {
	AlertTriangle,
	Check,
	ChevronDown,
	ChevronRight,
	Plus,
	RefreshCw,
	Server,
	Trash2,
	X,
	Zap,
} from "lucide-react";
import { useState } from "react";

const PROVIDER_TYPES = [
	{ value: "openai", label: "OpenAI" },
	{ value: "anthropic", label: "Anthropic" },
	{ value: "google", label: "Google (Gemini)" },
	{ value: "groq", label: "Groq" },
	{ value: "openrouter", label: "OpenRouter" },
	{ value: "mistral", label: "Mistral" },
	{ value: "xai", label: "xAI (Grok)" },
	{ value: "bedrock", label: "AWS Bedrock" },
	{ value: "azure", label: "Azure OpenAI" },
	{ value: "ollama", label: "Ollama (local)" },
	{ value: "openai-compatible", label: "OpenAI-compatible" },
];

function AddProviderDialog({
	onClose,
	onSubmit,
	isPending,
}: {
	onClose: () => void;
	onSubmit: (data: {
		name: string;
		type: string;
		api_key?: string;
		base_url?: string;
	}) => void;
	isPending: boolean;
}) {
	const [name, setName] = useState("");
	const [type_, setType] = useState("openai");
	const [apiKey, setApiKey] = useState("");
	const [baseUrl, setBaseUrl] = useState("");

	const handleSubmit = (e: React.FormEvent) => {
		e.preventDefault();
		onSubmit({
			name: name.toLowerCase().replace(/\s+/g, "-"),
			type: type_,
			api_key: apiKey || undefined,
			base_url: baseUrl || undefined,
		});
	};

	return (
		<div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50">
			<div className="bg-card border border-border p-6 w-full max-w-md">
				<h3 className="text-sm font-semibold mb-4">Add Provider</h3>
				<form onSubmit={handleSubmit} className="space-y-3">
					<div>
						<label className="text-xs text-muted-foreground block mb-1">
							Provider Name
						</label>
						<input
							type="text"
							value={name}
							onChange={(e) => setName(e.target.value)}
							placeholder="e.g. anthropic, my-openai"
							className="w-full px-3 py-2 text-sm bg-background border border-border focus:outline-none focus:ring-1 focus:ring-ring"
							required
						/>
					</div>
					<div>
						<label className="text-xs text-muted-foreground block mb-1">
							Type
						</label>
						<select
							value={type_}
							onChange={(e) => setType(e.target.value)}
							className="w-full px-3 py-2 text-sm bg-background border border-border focus:outline-none focus:ring-1 focus:ring-ring"
						>
							{PROVIDER_TYPES.map((t) => (
								<option key={t.value} value={t.value}>
									{t.label}
								</option>
							))}
						</select>
					</div>
					<div>
						<label className="text-xs text-muted-foreground block mb-1">
							API Key
						</label>
						<input
							type="password"
							value={apiKey}
							onChange={(e) => setApiKey(e.target.value)}
							placeholder="sk-..."
							className="w-full px-3 py-2 text-sm bg-background border border-border focus:outline-none focus:ring-1 focus:ring-ring font-mono"
						/>
					</div>
					<div>
						<label className="text-xs text-muted-foreground block mb-1">
							Base URL{" "}
							<span className="text-muted-foreground/60">(optional)</span>
						</label>
						<input
							type="url"
							value={baseUrl}
							onChange={(e) => setBaseUrl(e.target.value)}
							placeholder="https://api.example.com/v1"
							className="w-full px-3 py-2 text-sm bg-background border border-border focus:outline-none focus:ring-1 focus:ring-ring font-mono"
						/>
					</div>
					<div className="flex justify-end gap-2 pt-2">
						<Button
							type="button"
							variant="ghost"
							size="sm"
							onClick={onClose}
						>
							Cancel
						</Button>
						<Button type="submit" size="sm" disabled={!name || isPending}>
							{isPending ? (
								<RefreshCw className="w-3 h-3 mr-1 animate-spin" />
							) : (
								<Plus className="w-3 h-3 mr-1" />
							)}
							Add Provider
						</Button>
					</div>
				</form>
			</div>
		</div>
	);
}

function ProviderCard({
	provider,
	onDelete,
	isDeleting,
}: {
	provider: EavsProviderSummary;
	onDelete: (name: string) => void;
	isDeleting: boolean;
}) {
	const [expanded, setExpanded] = useState(false);

	return (
		<div className="border border-border">
			<div className="flex items-center">
				<button
					type="button"
					onClick={() => setExpanded(!expanded)}
					className="flex-1 flex items-center justify-between px-4 py-3 hover:bg-muted/30 transition-colors"
				>
					<div className="flex items-center gap-3">
						<Server className="w-4 h-4 text-muted-foreground" />
						<div className="text-left">
							<div className="text-sm font-medium text-foreground">
								{provider.name}
							</div>
							<div className="text-xs text-muted-foreground">
								{provider.type}
								{provider.pi_api ? ` (${provider.pi_api})` : ""}
							</div>
						</div>
					</div>
					<div className="flex items-center gap-2">
						<Badge
							variant={provider.has_api_key ? "default" : "outline"}
							className="text-[10px]"
						>
							{provider.has_api_key ? "configured" : "no key"}
						</Badge>
						<Badge variant="secondary" className="text-[10px]">
							{provider.model_count} models
						</Badge>
						{expanded ? (
							<ChevronDown className="w-4 h-4 text-muted-foreground" />
						) : (
							<ChevronRight className="w-4 h-4 text-muted-foreground" />
						)}
					</div>
				</button>
				<Button
					variant="ghost"
					size="sm"
					onClick={() => onDelete(provider.name)}
					disabled={isDeleting}
					className="h-8 w-8 p-0 mr-2 text-muted-foreground hover:text-destructive"
					title={`Delete ${provider.name}`}
				>
					<Trash2 className="w-3.5 h-3.5" />
				</Button>
			</div>
			{expanded && provider.models.length > 0 && (
				<div className="border-t border-border px-4 py-2 max-h-64 overflow-y-auto">
					<table className="w-full text-xs">
						<thead>
							<tr className="text-muted-foreground">
								<th className="text-left py-1 font-medium">Model ID</th>
								<th className="text-left py-1 font-medium">Name</th>
								<th className="text-left py-1 font-medium">Reasoning</th>
							</tr>
						</thead>
						<tbody>
							{provider.models.map((model) => (
								<tr
									key={model.id}
									className="border-t border-border/50 text-muted-foreground"
								>
									<td className="py-1 font-mono">{model.id}</td>
									<td className="py-1">{model.name || "-"}</td>
									<td className="py-1">
										{model.reasoning ? (
											<Zap className="w-3 h-3 text-amber-500" />
										) : (
											"-"
										)}
									</td>
								</tr>
							))}
						</tbody>
					</table>
				</div>
			)}
		</div>
	);
}

export function ModelsPanel() {
	const { data: eavsData, isLoading, error, refetch } = useEavsProviders();
	const upsertMutation = useUpsertEavsProvider();
	const deleteMutation = useDeleteEavsProvider();
	const syncModelsMutation = useSyncAllModels();
	const [showAddDialog, setShowAddDialog] = useState(false);
	const [syncResult, setSyncResult] = useState<SyncAllModelsResponse | null>(
		null,
	);

	const handleAddProvider = async (data: {
		name: string;
		type: string;
		api_key?: string;
		base_url?: string;
	}) => {
		await upsertMutation.mutateAsync(data);
		setShowAddDialog(false);
	};

	const handleDelete = async (name: string) => {
		if (!window.confirm(`Delete provider "${name}"?`)) return;
		await deleteMutation.mutateAsync(name);
	};

	const handleSyncModels = async () => {
		setSyncResult(null);
		const result = await syncModelsMutation.mutateAsync();
		setSyncResult(result);
	};

	if (error) {
		return (
			<div className="bg-card border border-border">
				<div className="border-b border-border px-4 py-3 flex items-center justify-between">
					<h2 className="text-xs font-semibold text-muted-foreground tracking-wider">
						MODEL PROVIDERS
					</h2>
					<Button
						variant="ghost"
						size="sm"
						onClick={() => refetch()}
						className="h-7"
					>
						<RefreshCw className="w-3 h-3" />
					</Button>
				</div>
				<div className="p-4 text-sm text-destructive flex items-center gap-2">
					<AlertTriangle className="w-4 h-4" />
					{error instanceof Error ? error.message : "Failed to load providers"}
				</div>
			</div>
		);
	}

	return (
		<div className="space-y-4">
			{/* Providers */}
			<div className="bg-card border border-border">
				<div className="border-b border-border px-4 py-3 flex items-center justify-between">
					<div className="flex items-center gap-3">
						<h2 className="text-xs font-semibold text-muted-foreground tracking-wider">
							MODEL PROVIDERS
						</h2>
						{eavsData && (
							<Badge variant="secondary" className="text-[10px]">
								{eavsData.providers.length} providers
							</Badge>
						)}
					</div>
					<div className="flex items-center gap-2">
						<Button
							variant="ghost"
							size="sm"
							onClick={() => refetch()}
							className="h-7"
						>
							<RefreshCw className="w-3 h-3" />
						</Button>
						<Button
							size="sm"
							onClick={() => setShowAddDialog(true)}
							className="h-7"
						>
							<Plus className="w-3 h-3 mr-1" />
							Add Provider
						</Button>
					</div>
				</div>

				{isLoading ? (
					<div className="p-4 text-sm text-muted-foreground">
						Loading providers...
					</div>
				) : eavsData && eavsData.providers.length > 0 ? (
					<div className="p-3 space-y-2">
						{eavsData.providers.map((provider) => (
							<ProviderCard
								key={provider.name}
								provider={provider}
								onDelete={handleDelete}
								isDeleting={deleteMutation.isPending}
							/>
						))}
					</div>
				) : (
					<div className="p-8 text-center text-sm text-muted-foreground">
						No providers configured. Click "Add Provider" to get started.
					</div>
				)}

				{eavsData && (
					<div className="border-t border-border px-4 py-2 text-xs text-muted-foreground">
						EAVS endpoint: {eavsData.eavs_url}
					</div>
				)}
			</div>

			{/* Sync Models Section */}
			<div className="bg-card border border-border">
				<div className="border-b border-border px-4 py-3 flex items-center justify-between">
					<div>
						<h2 className="text-xs font-semibold text-muted-foreground tracking-wider">
							DEPLOY MODELS TO USERS
						</h2>
						<p className="text-xs text-muted-foreground mt-1">
							After adding or changing providers, sync models.json to all users
						</p>
					</div>
					<Button
						size="sm"
						onClick={handleSyncModels}
						disabled={syncModelsMutation.isPending}
						className="h-7"
					>
						{syncModelsMutation.isPending ? (
							<RefreshCw className="w-3 h-3 mr-1 animate-spin" />
						) : (
							<RefreshCw className="w-3 h-3 mr-1" />
						)}
						Sync Models to All Users
					</Button>
				</div>

				{syncModelsMutation.error && (
					<div className="p-3 text-sm text-destructive flex items-center gap-2">
						<AlertTriangle className="w-4 h-4" />
						{syncModelsMutation.error instanceof Error
							? syncModelsMutation.error.message
							: "Sync failed"}
					</div>
				)}

				{syncResult && (
					<div className="p-3">
						<div className="flex items-center gap-2 text-sm">
							{syncResult.ok ? (
								<Check className="w-4 h-4 text-green-500" />
							) : (
								<AlertTriangle className="w-4 h-4 text-amber-500" />
							)}
							<span>
								Synced {syncResult.synced} of {syncResult.total} users
							</span>
						</div>
						{syncResult.errors.length > 0 && (
							<div className="mt-2 space-y-1">
								{syncResult.errors.map((err) => (
									<div
										key={err}
										className="text-xs text-destructive flex items-center gap-1"
									>
										<X className="w-3 h-3" />
										{err}
									</div>
								))}
							</div>
						)}
					</div>
				)}
			</div>

			{/* Add Provider Dialog */}
			{showAddDialog && (
				<AddProviderDialog
					onClose={() => setShowAddDialog(false)}
					onSubmit={handleAddProvider}
					isPending={upsertMutation.isPending}
				/>
			)}
		</div>
	);
}
