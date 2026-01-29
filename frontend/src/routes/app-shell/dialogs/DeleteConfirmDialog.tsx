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
import { memo } from "react";

export interface DeleteConfirmDialogProps {
	open: boolean;
	onOpenChange: (open: boolean) => void;
	onConfirm: () => void;
	locale: string;
	title?: string;
	description?: string;
}

export const DeleteConfirmDialog = memo(function DeleteConfirmDialog({
	open,
	onOpenChange,
	onConfirm,
	locale,
	title,
	description,
}: DeleteConfirmDialogProps) {
	return (
		<AlertDialog open={open} onOpenChange={onOpenChange}>
			<AlertDialogContent>
				<AlertDialogHeader>
					<AlertDialogTitle>
						{title ?? (locale === "de" ? "Chat loschen?" : "Delete chat?")}
					</AlertDialogTitle>
					<AlertDialogDescription>
						{description ??
							(locale === "de"
								? "Diese Aktion kann nicht ruckgangig gemacht werden. Der Chat wird dauerhaft geloscht."
								: "This action cannot be undone. The chat will be permanently deleted.")}
					</AlertDialogDescription>
				</AlertDialogHeader>
				<AlertDialogFooter>
					<AlertDialogCancel>
						{locale === "de" ? "Abbrechen" : "Cancel"}
					</AlertDialogCancel>
					<AlertDialogAction onClick={onConfirm}>
						{locale === "de" ? "Loschen" : "Delete"}
					</AlertDialogAction>
				</AlertDialogFooter>
			</AlertDialogContent>
		</AlertDialog>
	);
});
