"use client";

import {
	Dialog,
	DialogContent,
	DialogDescription,
	DialogFooter,
	DialogHeader,
	DialogTitle,
} from "@/components/ui/dialog";
import type { AppCapabilityRequest } from "@/src/generated/AppCapabilityRequest";
import type { AppPermissionRequest } from "@/src/generated/AppPermissionRequest";
import {
	ChevronDown,
	Database,
	FileImage,
	FolderOpen,
	Palette,
	Play,
} from "lucide-react";
import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";

interface AppPermissionDialogProps {
	request: AppPermissionRequest | null;
	busy: boolean;
	error: string | null;
	onAllow: () => void;
	onNotNow: () => void;
	onClose: () => void;
}

function capabilityKey(capability: AppCapabilityRequest): string {
	return capability.capability;
}

function capabilityIcon(capability: AppCapabilityRequest) {
	switch (capability.capability) {
		case "files":
			return <FolderOpen className="size-4" aria-hidden="true" />;
		case "operations":
			return <Play className="size-4" aria-hidden="true" />;
		case "theme":
			return <Palette className="size-4" aria-hidden="true" />;
		case "kv":
			return <Database className="size-4" aria-hidden="true" />;
	}
}

function fileSummary(
	capability: Extract<AppCapabilityRequest, { capability: "files" }>,
	t: (key: string, options?: Record<string, unknown>) => string,
): string {
	const writes = capability.resources.filter(
		(resource) => resource.access === "read_write",
	).length;
	if (writes === 0) return t("apps.permissions.filesReadOnly");
	return t("apps.permissions.filesReadWrite");
}

function capabilitySummary(
	capability: AppCapabilityRequest,
	t: (key: string, options?: Record<string, unknown>) => string,
): string {
	switch (capability.capability) {
		case "files":
			return fileSummary(capability, t);
		case "operations":
			return t("apps.permissions.operationsSummary", {
				count: capability.operations.length,
			});
		case "theme":
			return t("apps.permissions.themeSummary");
		case "kv":
			return t("apps.permissions.kvSummary");
	}
}

function capabilityTitle(
	capability: AppCapabilityRequest,
	t: (key: string) => string,
): string {
	return t(`apps.permissions.capability.${capability.capability}`);
}

function CapabilityDetails({
	capability,
}: {
	capability: AppCapabilityRequest;
}) {
	const { t } = useTranslation();
	if (capability.capability === "files") {
		return (
			<ul className="mt-2 space-y-1.5 border-t border-border pt-2 font-mono text-[11px] text-muted-foreground">
				{capability.resources.map((resource) => (
					<li key={resource.role} className="flex flex-wrap gap-x-2">
						<span className="text-foreground">{resource.path}</span>
						<span>
							{resource.access === "read_write"
								? t("apps.permissions.readWrite")
								: t("apps.permissions.readOnly")}
							{resource.watch ? ` · ${t("apps.permissions.watchChanges")}` : ""}
						</span>
					</li>
				))}
			</ul>
		);
	}
	if (capability.capability === "operations") {
		return (
			<ul className="mt-2 space-y-1.5 border-t border-border pt-2 text-[11px] text-muted-foreground">
				{capability.operations.map((operation) => (
					<li key={operation.id}>
						<span className="font-mono text-foreground">{operation.id}</span>
						{operation.summary ? ` — ${operation.summary}` : ""}
					</li>
				))}
			</ul>
		);
	}
	return null;
}

export function AppPermissionDialog({
	request,
	busy,
	error,
	onAllow,
	onNotNow,
	onClose,
}: AppPermissionDialogProps) {
	const { t } = useTranslation();
	const [detailsOpen, setDetailsOpen] = useState(false);
	const capabilities = useMemo(() => request?.capabilities ?? [], [request]);

	return (
		<Dialog
			open={request !== null}
			onOpenChange={(open) => {
				if (!open && !busy) onClose();
			}}
		>
			<DialogContent
				className="max-h-[calc(100dvh-2rem)] overflow-y-auto rounded-none border-border p-0 shadow-2xl sm:max-w-xl"
				showCloseButton={!busy}
			>
				{request && (
					<>
						<DialogHeader className="border-b border-border px-5 py-4 pr-12 text-left">
							<div className="flex items-center gap-2 text-xs text-muted-foreground">
								<FileImage className="size-4" aria-hidden="true" />
								<span>{t("apps.permissions.appRequest")}</span>
								<span aria-hidden="true">·</span>
								<span className="font-mono">v{request.version}</span>
							</div>
							<DialogTitle className="text-base leading-snug">
								{t("apps.permissions.title", { app: request.title.en })}
							</DialogTitle>
							<DialogDescription className="text-xs leading-relaxed">
								{t("apps.permissions.description")}
							</DialogDescription>
						</DialogHeader>

						<div className="space-y-2 px-5 py-4">
							{capabilities.map((capability) => (
								<div
									key={capabilityKey(capability)}
									className="border border-border bg-muted/20 px-3 py-2.5"
								>
									<div className="flex gap-3">
										<div className="mt-0.5 text-primary">
											{capabilityIcon(capability)}
										</div>
										<div className="min-w-0">
											<div className="text-xs font-medium">
												{capabilityTitle(capability, t)}
											</div>
											<p className="mt-0.5 text-xs leading-relaxed text-muted-foreground">
												{capabilitySummary(capability, t)}
											</p>
										</div>
									</div>
									{detailsOpen && <CapabilityDetails capability={capability} />}
								</div>
							))}

							<p className="text-xs leading-relaxed text-muted-foreground">
								{t("apps.permissions.agentContext")}
							</p>

							<button
								type="button"
								onClick={() => setDetailsOpen((open) => !open)}
								className="inline-flex min-h-11 items-center gap-2 text-xs text-muted-foreground hover:text-foreground"
								aria-expanded={detailsOpen}
							>
								<ChevronDown
									className={`size-4 transition-transform ${detailsOpen ? "rotate-180" : ""}`}
								/>
								{detailsOpen
									? t("apps.permissions.hideDetails")
									: t("apps.permissions.showDetails")}
							</button>

							{detailsOpen && (
								<div className="border-l-2 border-border pl-3 font-mono text-[10px] leading-relaxed text-muted-foreground">
									<div>{request.app_id}</div>
									<div>{request.content_digest.slice(0, 16)}…</div>
								</div>
							)}

							{error && (
								<div className="border border-destructive/50 bg-destructive/10 p-2 text-xs text-destructive">
									{error}
								</div>
							)}
						</div>

						<DialogFooter className="border-t border-border px-5 py-4 sm:justify-between">
							<button
								type="button"
								onClick={onNotNow}
								disabled={busy}
								className="min-h-11 px-4 text-sm text-muted-foreground hover:text-foreground disabled:opacity-50"
							>
								{t("apps.permissions.notNow")}
							</button>
							<button
								type="button"
								onClick={onAllow}
								disabled={busy}
								className="min-h-11 border border-primary bg-primary px-5 text-sm font-medium text-primary-foreground disabled:opacity-50"
							>
								{busy
									? t("apps.permissions.saving")
									: t("apps.permissions.allow")}
							</button>
						</DialogFooter>
					</>
				)}
			</DialogContent>
		</Dialog>
	);
}
