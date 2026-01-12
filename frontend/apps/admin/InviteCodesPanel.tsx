"use client";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
	Dialog,
	DialogContent,
	DialogDescription,
	DialogFooter,
	DialogHeader,
	DialogTitle,
} from "@/components/ui/dialog";
import {
	DropdownMenu,
	DropdownMenuContent,
	DropdownMenuItem,
	DropdownMenuSeparator,
	DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Skeleton } from "@/components/ui/skeleton";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import {
	Tooltip,
	TooltipContent,
	TooltipProvider,
	TooltipTrigger,
} from "@/components/ui/tooltip";
import {
	type BatchCreateInviteCodesRequest,
	type CreateInviteCodeRequest,
	type InviteCode,
	useCreateInviteCode,
	useCreateInviteCodesBatch,
	useDeleteInviteCode,
	useInviteCodeStats,
	useInviteCodes,
	useRevokeInviteCode,
} from "@/hooks/use-admin";
import {
	AlertTriangle,
	Check,
	Clock,
	Copy,
	MoreVertical,
	Plus,
	RefreshCw,
	Ticket,
	Trash2,
	X,
} from "lucide-react";
import { useState } from "react";

function ValidityBadge({ isValid }: { isValid: boolean }) {
	return (
		<Badge
			variant={isValid ? "default" : "outline"}
			className="text-[10px] gap-1"
		>
			{isValid ? <Check className="w-3 h-3" /> : <X className="w-3 h-3" />}
			{isValid ? "valid" : "invalid"}
		</Badge>
	);
}

function formatDate(dateStr: string | null): string {
	if (!dateStr) return "Never";
	const date = new Date(dateStr);
	return date.toLocaleDateString(undefined, {
		year: "numeric",
		month: "short",
		day: "numeric",
	});
}

function formatExpiry(expiresAt: string | null): string {
	if (!expiresAt) return "No expiry";
	const date = new Date(expiresAt);
	const now = new Date();
	if (date < now) return "Expired";
	return formatDate(expiresAt);
}

function CopyButton({ text }: { text: string }) {
	const [copied, setCopied] = useState(false);

	const handleCopy = async () => {
		await navigator.clipboard.writeText(text);
		setCopied(true);
		setTimeout(() => setCopied(false), 2000);
	};

	return (
		<TooltipProvider>
			<Tooltip>
				<TooltipTrigger asChild>
					<Button
						variant="ghost"
						size="sm"
						className="h-6 w-6 p-0"
						onClick={handleCopy}
					>
						{copied ? (
							<Check className="w-3 h-3 text-green-500" />
						) : (
							<Copy className="w-3 h-3" />
						)}
					</Button>
				</TooltipTrigger>
				<TooltipContent>
					<p>{copied ? "Copied!" : "Copy code"}</p>
				</TooltipContent>
			</Tooltip>
		</TooltipProvider>
	);
}

type CreateDialogProps = {
	open: boolean;
	onOpenChange: (open: boolean) => void;
	onCreateSingle: (data: CreateInviteCodeRequest) => Promise<void>;
	onCreateBatch: (data: BatchCreateInviteCodesRequest) => Promise<void>;
	isLoading: boolean;
};

function CreateDialog({
	open,
	onOpenChange,
	onCreateSingle,
	onCreateBatch,
	isLoading,
}: CreateDialogProps) {
	const [mode, setMode] = useState<"single" | "batch">("single");
	const [code, setCode] = useState("");
	const [maxUses, setMaxUses] = useState("1");
	const [expiresInHours, setExpiresInHours] = useState("");
	const [note, setNote] = useState("");
	const [count, setCount] = useState("5");
	const [prefix, setPrefix] = useState("");
	const [error, setError] = useState<string | null>(null);

	const handleSubmit = async (e: React.FormEvent) => {
		e.preventDefault();
		setError(null);

		try {
			const expiresInSecs = expiresInHours
				? Number.parseInt(expiresInHours, 10) * 3600
				: undefined;

			if (mode === "single") {
				await onCreateSingle({
					code: code || undefined,
					max_uses: Number.parseInt(maxUses, 10),
					expires_in_secs: expiresInSecs,
					note: note || undefined,
				});
			} else {
				await onCreateBatch({
					count: Number.parseInt(count, 10),
					uses_per_code: Number.parseInt(maxUses, 10),
					expires_in_secs: expiresInSecs,
					prefix: prefix || undefined,
					note: note || undefined,
				});
			}
			onOpenChange(false);
			// Reset form
			setCode("");
			setMaxUses("1");
			setExpiresInHours("");
			setNote("");
			setCount("5");
			setPrefix("");
		} catch (err) {
			setError(err instanceof Error ? err.message : "An error occurred");
		}
	};

	return (
		<Dialog open={open} onOpenChange={onOpenChange}>
			<DialogContent>
				<form onSubmit={handleSubmit}>
					<DialogHeader>
						<DialogTitle>Create Invite Code(s)</DialogTitle>
						<DialogDescription>
							Generate invite codes for new user registration.
						</DialogDescription>
					</DialogHeader>

					<Tabs
						value={mode}
						onValueChange={(v) => setMode(v as "single" | "batch")}
					>
						<TabsList className="w-full mt-4">
							<TabsTrigger value="single" className="flex-1">
								Single Code
							</TabsTrigger>
							<TabsTrigger value="batch" className="flex-1">
								Batch Create
							</TabsTrigger>
						</TabsList>

						<TabsContent value="single" className="space-y-4 mt-4">
							<div className="grid gap-2">
								<Label htmlFor="code">Custom Code (optional)</Label>
								<Input
									id="code"
									value={code}
									onChange={(e) => setCode(e.target.value)}
									placeholder="Leave empty for auto-generated"
								/>
							</div>
						</TabsContent>

						<TabsContent value="batch" className="space-y-4 mt-4">
							<div className="grid gap-2">
								<Label htmlFor="count">Number of Codes</Label>
								<Input
									id="count"
									type="number"
									min="1"
									max="100"
									value={count}
									onChange={(e) => setCount(e.target.value)}
								/>
							</div>
							<div className="grid gap-2">
								<Label htmlFor="prefix">Code Prefix (optional)</Label>
								<Input
									id="prefix"
									value={prefix}
									onChange={(e) => setPrefix(e.target.value)}
									placeholder="e.g., PROMO-"
								/>
							</div>
						</TabsContent>
					</Tabs>

					<div className="grid gap-4 py-4">
						<div className="grid gap-2">
							<Label htmlFor="maxUses">Uses Per Code</Label>
							<Input
								id="maxUses"
								type="number"
								min="1"
								value={maxUses}
								onChange={(e) => setMaxUses(e.target.value)}
							/>
						</div>

						<div className="grid gap-2">
							<Label htmlFor="expiresIn">Expires In (hours, optional)</Label>
							<Input
								id="expiresIn"
								type="number"
								min="1"
								value={expiresInHours}
								onChange={(e) => setExpiresInHours(e.target.value)}
								placeholder="No expiry"
							/>
						</div>

						<div className="grid gap-2">
							<Label htmlFor="note">Note (optional)</Label>
							<Input
								id="note"
								value={note}
								onChange={(e) => setNote(e.target.value)}
								placeholder="Admin note..."
							/>
						</div>

						{error && (
							<div className="text-sm text-destructive flex items-center gap-2">
								<AlertTriangle className="w-4 h-4" />
								{error}
							</div>
						)}
					</div>

					<DialogFooter>
						<Button
							type="button"
							variant="outline"
							onClick={() => onOpenChange(false)}
						>
							Cancel
						</Button>
						<Button type="submit" disabled={isLoading}>
							{isLoading && <RefreshCw className="w-4 h-4 mr-2 animate-spin" />}
							{mode === "single" ? "Create Code" : `Create ${count} Codes`}
						</Button>
					</DialogFooter>
				</form>
			</DialogContent>
		</Dialog>
	);
}

function CodeRow({
	code,
	onRevoke,
	onDelete,
	isActionLoading,
}: {
	code: InviteCode;
	onRevoke: () => void;
	onDelete: () => void;
	isActionLoading: boolean;
}) {
	return (
		<tr className="border-b border-border hover:bg-muted/30 transition">
			<td className="py-3 px-4">
				<div className="flex items-center gap-2">
					<code className="text-sm font-mono text-foreground bg-muted px-2 py-0.5 rounded">
						{code.code}
					</code>
					<CopyButton text={code.code} />
				</div>
			</td>
			<td className="py-3 px-4">
				<ValidityBadge isValid={code.is_valid} />
			</td>
			<td className="py-3 px-4">
				<span className="text-sm text-muted-foreground">
					{code.uses_remaining} / {code.max_uses}
				</span>
			</td>
			<td className="py-3 px-4">
				<span className="text-xs text-muted-foreground">
					{formatExpiry(code.expires_at)}
				</span>
			</td>
			<td className="py-3 px-4">
				<span className="text-xs text-muted-foreground">
					{formatDate(code.created_at)}
				</span>
			</td>
			<td className="py-3 px-4">
				{code.note && (
					<TooltipProvider>
						<Tooltip>
							<TooltipTrigger asChild>
								<span className="text-xs text-muted-foreground truncate max-w-[100px] block">
									{code.note}
								</span>
							</TooltipTrigger>
							<TooltipContent>
								<p>{code.note}</p>
							</TooltipContent>
						</Tooltip>
					</TooltipProvider>
				)}
			</td>
			<td className="py-3 px-4">
				<DropdownMenu>
					<DropdownMenuTrigger asChild>
						<Button
							variant="ghost"
							size="sm"
							className="h-7 w-7 p-0"
							disabled={isActionLoading}
						>
							{isActionLoading ? (
								<RefreshCw className="w-4 h-4 animate-spin" />
							) : (
								<MoreVertical className="w-4 h-4" />
							)}
						</Button>
					</DropdownMenuTrigger>
					<DropdownMenuContent align="end">
						{code.is_valid && (
							<>
								<DropdownMenuItem onClick={onRevoke}>
									<X className="w-4 h-4 mr-2" />
									Revoke
								</DropdownMenuItem>
								<DropdownMenuSeparator />
							</>
						)}
						<DropdownMenuItem
							onClick={onDelete}
							className="text-destructive focus:text-destructive"
						>
							<Trash2 className="w-4 h-4 mr-2" />
							Delete
						</DropdownMenuItem>
					</DropdownMenuContent>
				</DropdownMenu>
			</td>
		</tr>
	);
}

function MobileCodeCard({
	code,
	onRevoke,
	onDelete,
	isActionLoading,
}: {
	code: InviteCode;
	onRevoke: () => void;
	onDelete: () => void;
	isActionLoading: boolean;
}) {
	return (
		<div className="border border-border p-3 space-y-2">
			<div className="flex items-start justify-between gap-2">
				<div className="flex items-center gap-2">
					<code className="text-sm font-mono text-foreground bg-muted px-2 py-0.5 rounded">
						{code.code}
					</code>
					<CopyButton text={code.code} />
				</div>
				<DropdownMenu>
					<DropdownMenuTrigger asChild>
						<Button
							variant="ghost"
							size="sm"
							className="h-7 w-7 p-0 shrink-0"
							disabled={isActionLoading}
						>
							{isActionLoading ? (
								<RefreshCw className="w-4 h-4 animate-spin" />
							) : (
								<MoreVertical className="w-4 h-4" />
							)}
						</Button>
					</DropdownMenuTrigger>
					<DropdownMenuContent align="end">
						{code.is_valid && (
							<>
								<DropdownMenuItem onClick={onRevoke}>
									<X className="w-4 h-4 mr-2" />
									Revoke
								</DropdownMenuItem>
								<DropdownMenuSeparator />
							</>
						)}
						<DropdownMenuItem
							onClick={onDelete}
							className="text-destructive focus:text-destructive"
						>
							<Trash2 className="w-4 h-4 mr-2" />
							Delete
						</DropdownMenuItem>
					</DropdownMenuContent>
				</DropdownMenu>
			</div>

			<div className="flex flex-wrap gap-2 items-center">
				<ValidityBadge isValid={code.is_valid} />
				<Badge variant="outline" className="text-[10px]">
					{code.uses_remaining} / {code.max_uses} uses
				</Badge>
			</div>

			<div className="flex gap-4 text-xs text-muted-foreground">
				<span className="flex items-center gap-1">
					<Clock className="w-3 h-3" />
					{formatExpiry(code.expires_at)}
				</span>
				<span>Created: {formatDate(code.created_at)}</span>
			</div>

			{code.note && (
				<p className="text-xs text-muted-foreground truncate">{code.note}</p>
			)}
		</div>
	);
}

export function InviteCodesPanel() {
	const { data: codes, isLoading, error, refetch } = useInviteCodes();
	const { data: stats } = useInviteCodeStats();
	const createMutation = useCreateInviteCode();
	const createBatchMutation = useCreateInviteCodesBatch();
	const revokeMutation = useRevokeInviteCode();
	const deleteMutation = useDeleteInviteCode();

	const [dialogOpen, setDialogOpen] = useState(false);
	const [actionLoadingId, setActionLoadingId] = useState<string | null>(null);

	const handleCreateSingle = async (data: CreateInviteCodeRequest) => {
		await createMutation.mutateAsync(data);
	};

	const handleCreateBatch = async (data: BatchCreateInviteCodesRequest) => {
		await createBatchMutation.mutateAsync(data);
	};

	const handleRevoke = async (codeId: string) => {
		setActionLoadingId(codeId);
		try {
			await revokeMutation.mutateAsync(codeId);
		} finally {
			setActionLoadingId(null);
		}
	};

	const handleDelete = async (codeId: string) => {
		if (!confirm("Are you sure you want to delete this invite code?")) return;
		setActionLoadingId(codeId);
		try {
			await deleteMutation.mutateAsync(codeId);
		} finally {
			setActionLoadingId(null);
		}
	};

	if (error) {
		return (
			<div className="bg-card border border-border">
				<div className="border-b border-border px-3 md:px-4 py-2 md:py-3 flex items-center justify-between">
					<h2 className="text-xs md:text-sm font-semibold text-muted-foreground tracking-wider">
						INVITE CODES
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
					Failed to load invite codes: {error.message}
				</div>
			</div>
		);
	}

	return (
		<>
			<div className="bg-card border border-border">
				<div className="border-b border-border px-3 md:px-4 py-2 md:py-3 flex items-center justify-between">
					<div className="flex items-center gap-3">
						<h2 className="text-xs md:text-sm font-semibold text-muted-foreground tracking-wider">
							INVITE CODES
						</h2>
						{stats && (
							<div className="flex gap-2">
								<Badge variant="secondary" className="text-[10px]">
									{stats.valid} valid
								</Badge>
								<Badge variant="outline" className="text-[10px]">
									{stats.total} total
								</Badge>
							</div>
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
							onClick={() => setDialogOpen(true)}
							className="h-7"
						>
							<Plus className="w-3 h-3 mr-1" />
							Create
						</Button>
					</div>
				</div>

				{isLoading ? (
					<div className="p-4 space-y-3">
						<Skeleton className="h-12 w-full" />
						<Skeleton className="h-12 w-full" />
						<Skeleton className="h-12 w-full" />
					</div>
				) : codes && codes.length > 0 ? (
					<>
						{/* Mobile: Card Layout */}
						<div className="md:hidden p-3 space-y-3">
							{codes.map((code) => (
								<MobileCodeCard
									key={code.id}
									code={code}
									onRevoke={() => handleRevoke(code.id)}
									onDelete={() => handleDelete(code.id)}
									isActionLoading={actionLoadingId === code.id}
								/>
							))}
						</div>

						{/* Desktop: Table Layout */}
						<div className="hidden md:block p-4 overflow-x-auto">
							<table className="w-full min-w-[700px]">
								<thead>
									<tr className="border-b border-border">
										<th className="text-left py-2 px-4 text-xs font-medium text-muted-foreground tracking-wider">
											CODE
										</th>
										<th className="text-left py-2 px-4 text-xs font-medium text-muted-foreground tracking-wider">
											STATUS
										</th>
										<th className="text-left py-2 px-4 text-xs font-medium text-muted-foreground tracking-wider">
											USES
										</th>
										<th className="text-left py-2 px-4 text-xs font-medium text-muted-foreground tracking-wider">
											EXPIRES
										</th>
										<th className="text-left py-2 px-4 text-xs font-medium text-muted-foreground tracking-wider">
											CREATED
										</th>
										<th className="text-left py-2 px-4 text-xs font-medium text-muted-foreground tracking-wider">
											NOTE
										</th>
										<th className="text-left py-2 px-4 text-xs font-medium text-muted-foreground tracking-wider">
											ACTIONS
										</th>
									</tr>
								</thead>
								<tbody>
									{codes.map((code) => (
										<CodeRow
											key={code.id}
											code={code}
											onRevoke={() => handleRevoke(code.id)}
											onDelete={() => handleDelete(code.id)}
											isActionLoading={actionLoadingId === code.id}
										/>
									))}
								</tbody>
							</table>
						</div>
					</>
				) : (
					<div className="p-8 text-center text-sm text-muted-foreground">
						<Ticket className="w-8 h-8 mx-auto mb-2 opacity-50" />
						No invite codes found
					</div>
				)}
			</div>

			<CreateDialog
				open={dialogOpen}
				onOpenChange={setDialogOpen}
				onCreateSingle={handleCreateSingle}
				onCreateBatch={handleCreateBatch}
				isLoading={createMutation.isPending || createBatchMutation.isPending}
			/>
		</>
	);
}
