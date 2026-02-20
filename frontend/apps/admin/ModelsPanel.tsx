"use client";

import {
	AlertDialog,
	AlertDialogAction,
	AlertDialogCancel,
	AlertDialogContent,
	AlertDialogDescription,
	AlertDialogFooter,
	AlertDialogHeader,
	AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
	Dialog,
	DialogContent,
	DialogFooter,
	DialogHeader,
	DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
	Select,
	SelectContent,
	SelectItem,
	SelectTrigger,
	SelectValue,
} from "@/components/ui/select";
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
import { useCallback, useState } from "react";

const PROVIDER_TYPES = [
	{ value: "openai", label: "OpenAI" },
	{ value: "anthropic", label: "Anthropic" },
	{ value: "google", label: "Google (Gemini)" },
	{ value: "groq", label: "Groq" },
	{ value: "openrouter", label: "OpenRouter" },
	{ value: "mistral", label: "Mistral" },
	{ value: "xai", label: "xAI (Grok)" },
	{ value: "azure", label: "Azure OpenAI" },
	{ value: "openai-responses", label: "OpenAI Responses API" },
	{ value: "openai-compatible", label: "OpenAI-compatible" },
	{ value: "bedrock", label: "AWS Bedrock" },
	{ value: "ollama", label: "Ollama (local)" },
];

// Types that typically need a base_url
const TYPES_NEED_BASE_URL = ["azure", "openai-compatible", "ollama", "bedrock"];

// Types that support api_version
const TYPES_NEED_API_VERSION = ["azure"];

// Types that support deployment
const TYPES_NEED_DEPLOYMENT = ["azure"];

type ModelEntry = { id: string; name: string; reasoning: boolean };

// -- Model list editor (inline in the provider form) --
function ModelListEditor({
	models,
	onChange,
}: {
	models: ModelEntry[];
	onChange: (models: ModelEntry[]) => void;
}) {
	const [newId, setNewId] = useState("");
	const [newName, setNewName] = useState("");
	const [newReasoning, setNewReasoning] = useState(false);

	const addModel = useCallback(() => {
		const id = newId.trim();
		if (!id) return;
		if (models.some((m) => m.id === id)) return;
		onChange([
			...models,
			{ id, name: newName.trim() || id, reasoning: newReasoning },
		]);
		setNewId("");
		setNewName("");
		setNewReasoning(false);
	}, [models, newId, newName, newReasoning, onChange]);

	const removeModel = useCallback(
		(id: string) => {
			onChange(models.filter((m) => m.id !== id));
		},
		[models, onChange],
	);

	return (
		<div className="space-y-2">
			<Label>
				Models{" "}
				<span className="text-muted-foreground font-normal">
					(optional shortlist -- leave empty to use provider defaults)
				</span>
			</Label>

			{models.length > 0 && (
				<div className="border border-border rounded-md overflow-hidden">
					<table className="w-full text-xs">
						<thead>
							<tr className="bg-muted/30 text-muted-foreground">
								<th className="text-left py-1.5 px-2 font-medium">Model ID</th>
								<th className="text-left py-1.5 px-2 font-medium">Name</th>
								<th className="text-left py-1.5 px-2 font-medium w-16">
									Reasoning
								</th>
								<th className="w-8" />
							</tr>
						</thead>
						<tbody>
							{models.map((model) => (
								<tr
									key={model.id}
									className="border-t border-border/50 text-foreground"
								>
									<td className="py-1 px-2 font-mono">{model.id}</td>
									<td className="py-1 px-2">{model.name}</td>
									<td className="py-1 px-2 text-center">
										{model.reasoning ? (
											<Zap className="w-3 h-3 text-amber-500 inline" />
										) : (
											"-"
										)}
									</td>
									<td className="py-1 px-1">
										<button
											type="button"
											onClick={() => removeModel(model.id)}
											className="text-muted-foreground hover:text-destructive p-0.5"
										>
											<X className="w-3 h-3" />
										</button>
									</td>
								</tr>
							))}
						</tbody>
					</table>
				</div>
			)}

			<div className="flex gap-2 items-end">
				<div className="flex-1">
					<Input
						value={newId}
						onChange={(e) => setNewId(e.target.value)}
						placeholder="model-id"
						className="h-8 text-xs font-mono"
						onKeyDown={(e) => {
							if (e.key === "Enter") {
								e.preventDefault();
								addModel();
							}
						}}
					/>
				</div>
				<div className="flex-1">
					<Input
						value={newName}
						onChange={(e) => setNewName(e.target.value)}
						placeholder="Display name"
						className="h-8 text-xs"
						onKeyDown={(e) => {
							if (e.key === "Enter") {
								e.preventDefault();
								addModel();
							}
						}}
					/>
				</div>
				<label className="flex items-center gap-1 text-xs text-muted-foreground cursor-pointer whitespace-nowrap">
					<input
						type="checkbox"
						checked={newReasoning}
						onChange={(e) => setNewReasoning(e.target.checked)}
						className="rounded"
					/>
					Reasoning
				</label>
				<Button
					type="button"
					variant="outline"
					size="sm"
					className="h-8 px-2"
					onClick={addModel}
					disabled={!newId.trim()}
				>
					<Plus className="w-3 h-3" />
				</Button>
			</div>
		</div>
	);
}

// -- Add/Edit Provider Dialog --
function ProviderDialog({
	open,
	onOpenChange,
	onSubmit,
	isPending,
	initial,
}: {
	open: boolean;
	onOpenChange: (open: boolean) => void;
	onSubmit: (data: {
		name: string;
		type: string;
		api_key?: string;
		base_url?: string;
		api_version?: string;
		deployment?: string;
		models: ModelEntry[];
	}) => void;
	isPending: boolean;
	initial?: {
		name: string;
		type: string;
		models: ModelEntry[];
	};
}) {
	const isEdit = !!initial;
	const [name, setName] = useState(initial?.name ?? "");
	const [type_, setType] = useState(initial?.type ?? "openai");
	const [apiKey, setApiKey] = useState("");
	const [baseUrl, setBaseUrl] = useState("");
	const [apiVersion, setApiVersion] = useState("");
	const [deployment, setDeployment] = useState("");
	const [models, setModels] = useState<ModelEntry[]>(initial?.models ?? []);

	const resetForm = useCallback(() => {
		if (!initial) {
			setName("");
			setType("openai");
			setApiKey("");
			setBaseUrl("");
			setApiVersion("");
			setDeployment("");
			setModels([]);
		}
	}, [initial]);

	const handleSubmit = (e: React.FormEvent) => {
		e.preventDefault();
		onSubmit({
			name: isEdit ? name : name.toLowerCase().replace(/\s+/g, "-"),
			type: type_,
			api_key: apiKey || undefined,
			base_url: baseUrl || undefined,
			api_version: apiVersion || undefined,
			deployment: deployment || undefined,
			models,
		});
		resetForm();
	};

	const showBaseUrl = TYPES_NEED_BASE_URL.includes(type_) || baseUrl.length > 0;
	const showApiVersion = TYPES_NEED_API_VERSION.includes(type_);
	const showDeployment = TYPES_NEED_DEPLOYMENT.includes(type_);

	return (
		<Dialog open={open} onOpenChange={onOpenChange}>
			<DialogContent className="max-w-lg max-h-[85vh] overflow-y-auto">
				<DialogHeader>
					<DialogTitle>
						{isEdit ? `Edit ${initial.name}` : "Add Provider"}
					</DialogTitle>
				</DialogHeader>
				<form onSubmit={handleSubmit} className="space-y-4">
					{/* Name */}
					<div className="space-y-2">
						<Label htmlFor="provider-name">Provider Name</Label>
						<Input
							id="provider-name"
							value={name}
							onChange={(e) => setName(e.target.value)}
							placeholder="e.g. anthropic, azure-eastus"
							required
							disabled={isEdit}
							className={isEdit ? "opacity-50" : ""}
						/>
					</div>

					{/* Type */}
					<div className="space-y-2">
						<Label>Type</Label>
						<Select value={type_} onValueChange={setType}>
							<SelectTrigger className="w-full">
								<SelectValue placeholder="Select provider type" />
							</SelectTrigger>
							<SelectContent>
								{PROVIDER_TYPES.map((t) => (
									<SelectItem key={t.value} value={t.value}>
										{t.label}
									</SelectItem>
								))}
							</SelectContent>
						</Select>
					</div>

					{/* API Key */}
					<div className="space-y-2">
						<Label htmlFor="provider-key">
							API Key
							{isEdit && (
								<span className="text-muted-foreground font-normal">
									{" "}
									(leave blank to keep existing)
								</span>
							)}
						</Label>
						<Input
							id="provider-key"
							type="password"
							value={apiKey}
							onChange={(e) => setApiKey(e.target.value)}
							placeholder={isEdit ? "(unchanged)" : "sk-..."}
							className="font-mono"
						/>
					</div>

					{/* Base URL */}
					<div className="space-y-2">
						<Label htmlFor="provider-url">
							Base URL{" "}
							{!showBaseUrl && (
								<span className="text-muted-foreground font-normal">
									(auto-detected from type)
								</span>
							)}
							{showBaseUrl && (
								<span className="text-muted-foreground font-normal">
									(required for {type_})
								</span>
							)}
						</Label>
						<Input
							id="provider-url"
							value={baseUrl}
							onChange={(e) => setBaseUrl(e.target.value)}
							placeholder="https://api.example.com/v1"
							className="font-mono"
						/>
					</div>

					{/* API Version (Azure) */}
					{showApiVersion && (
						<div className="space-y-2">
							<Label htmlFor="provider-api-version">API Version</Label>
							<Input
								id="provider-api-version"
								value={apiVersion}
								onChange={(e) => setApiVersion(e.target.value)}
								placeholder="2024-12-01-preview"
								className="font-mono"
							/>
						</div>
					)}

					{/* Deployment (Azure) */}
					{showDeployment && (
						<div className="space-y-2">
							<Label htmlFor="provider-deployment">
								Deployment Name{" "}
								<span className="text-muted-foreground font-normal">
									(optional -- defaults to model name)
								</span>
							</Label>
							<Input
								id="provider-deployment"
								value={deployment}
								onChange={(e) => setDeployment(e.target.value)}
								placeholder="my-gpt4-deployment"
								className="font-mono"
							/>
						</div>
					)}

					{/* Models shortlist */}
					<ModelListEditor models={models} onChange={setModels} />

					<DialogFooter>
						<Button
							type="button"
							variant="outline"
							onClick={() => onOpenChange(false)}
						>
							Cancel
						</Button>
						<Button type="submit" disabled={!name || isPending}>
							{isPending ? (
								<RefreshCw className="w-3 h-3 mr-1 animate-spin" />
							) : isEdit ? (
								<Check className="w-3 h-3 mr-1" />
							) : (
								<Plus className="w-3 h-3 mr-1" />
							)}
							{isEdit ? "Save Changes" : "Add Provider"}
						</Button>
					</DialogFooter>
				</form>
			</DialogContent>
		</Dialog>
	);
}

function DeleteProviderDialog({
	providerName,
	open,
	onOpenChange,
	onConfirm,
	isPending,
}: {
	providerName: string;
	open: boolean;
	onOpenChange: (open: boolean) => void;
	onConfirm: () => void;
	isPending: boolean;
}) {
	return (
		<AlertDialog open={open} onOpenChange={onOpenChange}>
			<AlertDialogContent>
				<AlertDialogHeader>
					<AlertDialogTitle>Delete Provider</AlertDialogTitle>
					<AlertDialogDescription>
						This will remove the provider{" "}
						<span className="font-mono font-semibold text-foreground">
							{providerName}
						</span>{" "}
						from the EAVS configuration and delete its API key. This action
						cannot be undone.
					</AlertDialogDescription>
				</AlertDialogHeader>
				<AlertDialogFooter>
					<AlertDialogCancel disabled={isPending}>Cancel</AlertDialogCancel>
					<AlertDialogAction
						onClick={onConfirm}
						disabled={isPending}
						className="bg-destructive text-destructive-foreground hover:bg-destructive/90"
					>
						{isPending ? (
							<RefreshCw className="w-3 h-3 mr-1 animate-spin" />
						) : (
							<Trash2 className="w-3 h-3 mr-1" />
						)}
						Delete Provider
					</AlertDialogAction>
				</AlertDialogFooter>
			</AlertDialogContent>
		</AlertDialog>
	);
}

function ProviderCard({
	provider,
	onDelete,
	onEdit,
}: {
	provider: EavsProviderSummary;
	onDelete: (name: string) => void;
	onEdit: (provider: EavsProviderSummary) => void;
}) {
	const [expanded, setExpanded] = useState(false);

	return (
		<div className="border border-border rounded-md">
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
				<div className="flex items-center mr-2 gap-1">
					<Button
						variant="ghost"
						size="sm"
						onClick={() => onEdit(provider)}
						className="h-8 px-2 text-xs text-muted-foreground hover:text-foreground"
					>
						Edit
					</Button>
					<Button
						variant="ghost"
						size="sm"
						onClick={() => onDelete(provider.name)}
						className="h-8 w-8 p-0 text-muted-foreground hover:text-destructive"
						title={`Delete ${provider.name}`}
					>
						<Trash2 className="w-3.5 h-3.5" />
					</Button>
				</div>
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
			{expanded && provider.models.length === 0 && (
				<div className="border-t border-border px-4 py-3 text-xs text-muted-foreground">
					No model shortlist configured -- using provider defaults from
					models.dev catalog.
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
	const [editTarget, setEditTarget] = useState<EavsProviderSummary | null>(
		null,
	);
	const [deleteTarget, setDeleteTarget] = useState<string | null>(null);
	const [syncResult, setSyncResult] = useState<SyncAllModelsResponse | null>(
		null,
	);

	const handleUpsertProvider = async (data: {
		name: string;
		type: string;
		api_key?: string;
		base_url?: string;
		api_version?: string;
		deployment?: string;
		models: ModelEntry[];
	}) => {
		await upsertMutation.mutateAsync(data);
		setShowAddDialog(false);
		setEditTarget(null);
	};

	const handleDeleteConfirm = async () => {
		if (!deleteTarget) return;
		await deleteMutation.mutateAsync(deleteTarget);
		setDeleteTarget(null);
	};

	const handleSyncModels = async () => {
		setSyncResult(null);
		const result = await syncModelsMutation.mutateAsync();
		setSyncResult(result);
	};

	if (error) {
		return (
			<div className="bg-card border border-border rounded-md">
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
			<div className="bg-card border border-border rounded-md">
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
								onDelete={setDeleteTarget}
								onEdit={setEditTarget}
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
			<div className="bg-card border border-border rounded-md">
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
			<ProviderDialog
				open={showAddDialog}
				onOpenChange={setShowAddDialog}
				onSubmit={handleUpsertProvider}
				isPending={upsertMutation.isPending}
			/>

			{/* Edit Provider Dialog */}
			{editTarget && (
				<ProviderDialog
					open={true}
					onOpenChange={(open) => {
						if (!open) setEditTarget(null);
					}}
					onSubmit={handleUpsertProvider}
					isPending={upsertMutation.isPending}
					initial={{
						name: editTarget.name,
						type: editTarget.type,
						models: editTarget.models.map((m) => ({
							id: m.id,
							name: m.name,
							reasoning: m.reasoning,
						})),
					}}
				/>
			)}

			{/* Delete Confirmation Dialog */}
			<DeleteProviderDialog
				providerName={deleteTarget ?? ""}
				open={deleteTarget !== null}
				onOpenChange={(open) => {
					if (!open) setDeleteTarget(null);
				}}
				onConfirm={handleDeleteConfirm}
				isPending={deleteMutation.isPending}
			/>
		</div>
	);
}
