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
	return (
		<Dialog open={open} onOpenChange={onOpenChange}>
			<DialogContent>
				<DialogHeader>
					<DialogTitle>
						{locale === "de" ? "Workspace benennen" : "Name your workspace"}
					</DialogTitle>
					<DialogDescription>
						{locale === "de"
							? "Geben Sie Ihrem ersten Workspace einen Anzeigenamen."
							: "Give your first workspace a display name."}
					</DialogDescription>
				</DialogHeader>

				<div className="grid gap-4 py-4">
					<div className="grid gap-2">
						<Label htmlFor="workspace-name">
							{locale === "de" ? "Anzeigename" : "Display name"}
						</Label>
						<Input
							id="workspace-name"
							placeholder={
								locale === "de" ? "z.B. Hauptprojekt" : "e.g. Main project"
							}
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
						{locale === "de" ? "Abbrechen" : "Cancel"}
					</Button>
					<Button onClick={onSubmit} disabled={loading || !name.trim()}>
						{loading && <Loader2 className="mr-2 h-4 w-4 animate-spin" />}
						{locale === "de" ? "Erstellen" : "Create"}
					</Button>
				</DialogFooter>
			</DialogContent>
		</Dialog>
	);
}
