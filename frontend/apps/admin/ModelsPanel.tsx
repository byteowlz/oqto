"use client";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
	type EavsProviderSummary,
	type SyncUserConfigResult,
	useEavsProviders,
	useSyncUserConfigs,
} from "@/hooks/use-admin";
import {
	AlertTriangle,
	Check,
	ChevronDown,
	ChevronRight,
	RefreshCw,
	Server,
	X,
	Zap,
} from "lucide-react";
import { useState } from "react";

function ProviderCard({ provider }: { provider: EavsProviderSummary }) {
	const [expanded, setExpanded] = useState(false);

	return (
		<div className="border border-border">
			<button
				type="button"
				onClick={() => setExpanded(!expanded)}
				className="w-full flex items-center justify-between px-4 py-3 hover:bg-muted/30 transition-colors"
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

function SyncResultRow({ result }: { result: SyncUserConfigResult }) {
	return (
		<tr className="border-b border-border/50 text-xs">
			<td className="py-2 px-3 font-medium text-foreground">
				{result.user_id}
			</td>
			<td className="py-2 px-3 text-muted-foreground">
				{result.linux_username || "-"}
			</td>
			<td className="py-2 px-3">
				{result.eavs_configured ? (
					<Check className="w-3.5 h-3.5 text-green-500" />
				) : (
					<X className="w-3.5 h-3.5 text-muted-foreground" />
				)}
			</td>
			<td className="py-2 px-3">
				{result.runner_configured ? (
					<Check className="w-3.5 h-3.5 text-green-500" />
				) : (
					<X className="w-3.5 h-3.5 text-muted-foreground" />
				)}
			</td>
			<td className="py-2 px-3">
				{result.error ? (
					<span className="text-destructive truncate max-w-[200px] inline-block">
						{result.error}
					</span>
				) : (
					<span className="text-green-500">OK</span>
				)}
			</td>
		</tr>
	);
}

export function ModelsPanel() {
	const {
		data: eavsData,
		isLoading,
		error,
		refetch,
	} = useEavsProviders();
	const syncMutation = useSyncUserConfigs();
	const [syncResults, setSyncResults] = useState<
		SyncUserConfigResult[] | null
	>(null);

	const handleSyncAll = async () => {
		setSyncResults(null);
		const result = await syncMutation.mutateAsync(undefined);
		setSyncResults(result.results);
	};

	if (error) {
		return (
			<div className="bg-card border border-border">
				<div className="border-b border-border px-4 py-3 flex items-center justify-between">
					<h2 className="text-xs font-semibold text-muted-foreground tracking-wider">
						MODEL PROVIDERS
					</h2>
					<Button variant="ghost" size="sm" onClick={() => refetch()} className="h-7">
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
					</div>
				</div>

				{isLoading ? (
					<div className="p-4 text-sm text-muted-foreground">
						Loading providers...
					</div>
				) : eavsData && eavsData.providers.length > 0 ? (
					<div className="p-3 space-y-2">
						{eavsData.providers.map((provider) => (
							<ProviderCard key={provider.name} provider={provider} />
						))}
					</div>
				) : (
					<div className="p-8 text-center text-sm text-muted-foreground">
						No providers configured. Add providers to EAVS to enable model
						access.
					</div>
				)}

				{eavsData && (
					<div className="border-t border-border px-4 py-2 text-xs text-muted-foreground">
						EAVS endpoint: {eavsData.eavs_url}
					</div>
				)}
			</div>

			{/* Sync Section */}
			<div className="bg-card border border-border">
				<div className="border-b border-border px-4 py-3 flex items-center justify-between">
					<div>
						<h2 className="text-xs font-semibold text-muted-foreground tracking-wider">
							DEPLOY MODELS TO USERS
						</h2>
						<p className="text-xs text-muted-foreground mt-1">
							Sync the current provider catalog to all users' models.json
						</p>
					</div>
					<Button
						size="sm"
						onClick={handleSyncAll}
						disabled={syncMutation.isPending}
						className="h-7"
					>
						{syncMutation.isPending ? (
							<RefreshCw className="w-3 h-3 mr-1 animate-spin" />
						) : (
							<RefreshCw className="w-3 h-3 mr-1" />
						)}
						Sync All Users
					</Button>
				</div>

				{syncMutation.error && (
					<div className="p-3 text-sm text-destructive flex items-center gap-2">
						<AlertTriangle className="w-4 h-4" />
						{syncMutation.error instanceof Error
							? syncMutation.error.message
							: "Sync failed"}
					</div>
				)}

				{syncResults && syncResults.length > 0 && (
					<div className="p-3 overflow-x-auto">
						<table className="w-full">
							<thead>
								<tr className="border-b border-border text-xs text-muted-foreground">
									<th className="text-left py-2 px-3 font-medium">User</th>
									<th className="text-left py-2 px-3 font-medium">
										Linux User
									</th>
									<th className="text-left py-2 px-3 font-medium">Models</th>
									<th className="text-left py-2 px-3 font-medium">Runner</th>
									<th className="text-left py-2 px-3 font-medium">Status</th>
								</tr>
							</thead>
							<tbody>
								{syncResults.map((result) => (
									<SyncResultRow key={result.user_id} result={result} />
								))}
							</tbody>
						</table>
					</div>
				)}
			</div>
		</div>
	);
}
