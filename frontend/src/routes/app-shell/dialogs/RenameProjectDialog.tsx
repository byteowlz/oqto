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
import { memo, useCallback, useEffect, useState } from "react";

export interface RenameProjectDialogProps {
	open: boolean;
	onOpenChange: (open: boolean) => void;
	initialValue: string;
	onConfirm: (newValue: string) => void;
	locale: string;
}

export const RenameProjectDialog = memo(function RenameProjectDialog({
	open,
	onOpenChange,
	initialValue,
	onConfirm,
	locale,
}: RenameProjectDialogProps) {
	const [value, setValue] = useState(initialValue);

	// Sync with initial value when dialog opens
	useEffect(() => {
		if (open) {
			setValue(initialValue);
		}
	}, [open, initialValue]);

	const handleConfirm = useCallback(() => {
		onConfirm(value);
	}, [onConfirm, value]);

	return (
		<Dialog open={open} onOpenChange={onOpenChange}>
			<DialogContent>
				<DialogHeader>
					<DialogTitle>
						{locale === "de" ? "Projekt umbenennen" : "Rename project"}
					</DialogTitle>
					<DialogDescription>
						{locale === "de"
							? "Geben Sie einen neuen Namen fur dieses Projekt ein."
							: "Enter a new name for this project."}
					</DialogDescription>
				</DialogHeader>
				<Input
					value={value}
					onChange={(e) => setValue(e.target.value)}
					placeholder={locale === "de" ? "Projektname" : "Project name"}
					onKeyDown={(e) => {
						if (e.key === "Enter") {
							handleConfirm();
						}
					}}
				/>
				<DialogFooter>
					<Button
						type="button"
						variant="outline"
						onClick={() => onOpenChange(false)}
					>
						{locale === "de" ? "Abbrechen" : "Cancel"}
					</Button>
					<Button type="button" onClick={handleConfirm}>
						{locale === "de" ? "Speichern" : "Save"}
					</Button>
				</DialogFooter>
			</DialogContent>
		</Dialog>
	);
});
