"use client";

import { Button } from "@/components/ui/button";
import {
	Dialog,
	DialogContent,
	DialogDescription,
	DialogFooter,
	DialogHeader,
	DialogTitle,
} from "@/components/ui/dialog";
import type { Permission, PermissionResponse } from "@/lib/opencode-client";
import { cn } from "@/lib/utils";
import { AlertTriangle, FileEdit, Globe, Shield, Terminal } from "lucide-react";
import { useCallback, useState } from "react";

interface PermissionDialogProps {
	permission: Permission | null;
	onRespond: (
		permissionId: string,
		response: PermissionResponse,
	) => Promise<void>;
	onDismiss: () => void;
}

// Map tool names to icons and descriptions
function getToolInfo(tool: string): {
	icon: React.ReactNode;
	category: string;
	riskLevel: "low" | "medium" | "high";
} {
	const toolLower = tool.toLowerCase();

	if (
		toolLower.includes("bash") ||
		toolLower.includes("shell") ||
		toolLower.includes("exec")
	) {
		return {
			icon: <Terminal className="w-5 h-5" />,
			category: "Shell Command",
			riskLevel: "high",
		};
	}
	if (
		toolLower.includes("edit") ||
		toolLower.includes("write") ||
		toolLower.includes("file")
	) {
		return {
			icon: <FileEdit className="w-5 h-5" />,
			category: "File Edit",
			riskLevel: "medium",
		};
	}
	if (
		toolLower.includes("web") ||
		toolLower.includes("fetch") ||
		toolLower.includes("http")
	) {
		return {
			icon: <Globe className="w-5 h-5" />,
			category: "Web Request",
			riskLevel: "low",
		};
	}

	return {
		icon: <Shield className="w-5 h-5" />,
		category: "Tool",
		riskLevel: "medium",
	};
}

function getRiskColor(risk: "low" | "medium" | "high"): string {
	switch (risk) {
		case "high":
			return "text-red-500 bg-red-500/10 border-red-500/30";
		case "medium":
			return "text-yellow-500 bg-yellow-500/10 border-yellow-500/30";
		case "low":
			return "text-green-500 bg-green-500/10 border-green-500/30";
	}
}

export function PermissionDialog({
	permission,
	onRespond,
	onDismiss,
}: PermissionDialogProps) {
	const [isResponding, setIsResponding] = useState(false);
	const [selectedResponse, setSelectedResponse] =
		useState<PermissionResponse | null>(null);

	const handleRespond = useCallback(
		async (response: PermissionResponse) => {
			if (!permission || isResponding) return;

			setIsResponding(true);
			setSelectedResponse(response);

			try {
				await onRespond(permission.id, response);
				onDismiss();
			} catch (err) {
				console.error("Failed to respond to permission:", err);
			} finally {
				setIsResponding(false);
				setSelectedResponse(null);
			}
		},
		[permission, onRespond, onDismiss, isResponding],
	);

	if (!permission) return null;

	// Use 'type' field for permission type (e.g., "bash", "edit")
	const toolInfo = getToolInfo(permission.type);
	const riskLevel = toolInfo.riskLevel;
	const riskColor = getRiskColor(riskLevel);

	return (
		<Dialog open={!!permission} onOpenChange={(open) => !open && onDismiss()}>
			<DialogContent className="sm:max-w-md" showCloseButton={false}>
				<DialogHeader>
					<div className="flex items-center gap-3 mb-2">
						<div className={cn("p-2 rounded-lg border", riskColor)}>
							{toolInfo.icon}
						</div>
						<div>
							<DialogTitle className="text-base">
								Permission Required
							</DialogTitle>
							<p className="text-xs text-muted-foreground mt-0.5">
								{toolInfo.category}
							</p>
						</div>
					</div>
					<DialogDescription className="text-left">
						{permission.title || `The agent wants to use ${permission.type}`}
					</DialogDescription>
				</DialogHeader>

				{/* Tool details */}
				<div className="space-y-3">
					{permission.pattern && (
						<div className="text-sm text-foreground bg-muted/50 border border-border p-3 rounded-lg font-mono">
							{Array.isArray(permission.pattern)
								? permission.pattern.join(", ")
								: permission.pattern}
						</div>
					)}

					{permission.metadata &&
						Object.keys(permission.metadata).length > 0 && (
							<div className="space-y-2">
								<p className="text-xs text-muted-foreground uppercase tracking-wide">
									Details
								</p>
								<pre className="text-xs bg-muted/50 border border-border p-3 rounded-lg overflow-x-auto max-h-32">
									{JSON.stringify(permission.metadata, null, 2)}
								</pre>
							</div>
						)}

					{/* Risk warning for high-risk actions */}
					{riskLevel === "high" && (
						<div className="flex items-start gap-2 p-3 bg-red-500/10 border border-red-500/30 rounded-lg">
							<AlertTriangle className="w-4 h-4 text-red-500 flex-shrink-0 mt-0.5" />
							<p className="text-xs text-red-500">
								This action can execute arbitrary commands on the system. Review
								carefully before approving.
							</p>
						</div>
					)}
				</div>

				<DialogFooter className="flex-col sm:flex-row gap-2">
					<div className="flex gap-2 w-full sm:w-auto">
						<Button
							type="button"
							variant="outline"
							size="sm"
							onClick={() => handleRespond("no")}
							disabled={isResponding}
							className="flex-1 sm:flex-none"
						>
							{isResponding && selectedResponse === "no" ? "..." : "Deny"}
						</Button>
						<Button
							type="button"
							variant="outline"
							size="sm"
							onClick={() => handleRespond("never")}
							disabled={isResponding}
							className="flex-1 sm:flex-none text-destructive hover:text-destructive"
						>
							{isResponding && selectedResponse === "never" ? "..." : "Never"}
						</Button>
					</div>
					<div className="flex gap-2 w-full sm:w-auto">
						<Button
							type="button"
							variant="outline"
							size="sm"
							onClick={() => handleRespond("always")}
							disabled={isResponding}
							className="flex-1 sm:flex-none text-primary hover:text-primary"
						>
							{isResponding && selectedResponse === "always" ? "..." : "Always"}
						</Button>
						<Button
							type="button"
							size="sm"
							onClick={() => handleRespond("yes")}
							disabled={isResponding}
							className="flex-1 sm:flex-none"
						>
							{isResponding && selectedResponse === "yes" ? "..." : "Allow"}
						</Button>
					</div>
				</DialogFooter>
			</DialogContent>
		</Dialog>
	);
}

// Banner component for showing pending permission count when dialog is not open
export function PermissionBanner({
	count,
	onClick,
}: {
	count: number;
	onClick: () => void;
}) {
	if (count === 0) return null;

	return (
		<button
			type="button"
			onClick={onClick}
			className="w-full flex items-center justify-between gap-2 px-3 py-2 bg-yellow-500/10 border border-yellow-500/30 text-yellow-500 hover:bg-yellow-500/20 transition-colors"
		>
			<div className="flex items-center gap-2">
				<Shield className="w-4 h-4" />
				<span className="text-sm font-medium">
					{count} permission{count !== 1 ? "s" : ""} pending
				</span>
			</div>
			<span className="text-xs">Click to review</span>
		</button>
	);
}
