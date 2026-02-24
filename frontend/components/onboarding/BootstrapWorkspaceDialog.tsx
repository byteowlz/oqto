import { Button } from "@/components/ui/button";
import {
	Dialog,
	DialogContent,
	DialogDescription,
	DialogFooter,
	DialogHeader,
	DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Loader2 } from "lucide-react";
import { useTranslation } from "react-i18next";

export function BootstrapWorkspaceDialog({
	open,
	onOpenChange,
	name,
	onNameChange,
	onSubmit,
	loading,
	error,
	locale,
}: {
	open: boolean;
	onOpenChange: (open: boolean) => void;
	name: string;
	onNameChange: (name: string) => void;
	onSubmit: () => void;
	loading: boolean;
	error: string | null;
	locale: "en" | "de";
}) {
	const { t } = useTranslation();
	return (
		<Dialog open={open} onOpenChange={onOpenChange}>
			<DialogContent>
				<DialogHeader>
					<DialogTitle>
						{t("dialogs.nameWorkspace")}
					</DialogTitle>
					<DialogDescription>
						{t("dialogs.nameWorkspaceDescription")}
					</DialogDescription>
				</DialogHeader>

				<div className="grid gap-4 py-4">
					<div className="grid gap-2">
						<Label htmlFor="workspace-name">
							{t("dialogs.displayName")}
						</Label>
						<Input
							id="workspace-name"
							placeholder={t("dialogs.displayNamePlaceholder")}
							value={name}
							onChange={(e) => onNameChange(e.target.value)}
							onKeyDown={(e) => {
								if (e.key === "Enter" && !loading) {
									onSubmit();
								}
							}}
							disabled={loading}
						/>
					</div>

					{error && <p className="text-sm text-destructive">{error}</p>}
				</div>

				<DialogFooter>
					<Button
						variant="outline"
						onClick={() => onOpenChange(false)}
						disabled={loading}
					>
						{t("common.cancel")}
					</Button>
					<Button onClick={onSubmit} disabled={loading || !name.trim()}>
						{loading && <Loader2 className="mr-2 h-4 w-4 animate-spin" />}
						{t("common.create")}
					</Button>
				</DialogFooter>
			</DialogContent>
		</Dialog>
	);
}
