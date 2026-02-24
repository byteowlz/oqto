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
import { useTranslation } from "react-i18next";

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
	const { t } = useTranslation();

	return (
		<AlertDialog open={open} onOpenChange={onOpenChange}>
			<AlertDialogContent>
				<AlertDialogHeader>
					<AlertDialogTitle>
						{title ?? t("sessions.deleteChatTitle")}
					</AlertDialogTitle>
					<AlertDialogDescription>
						{description ?? t("sessions.deleteChatDescription")}
					</AlertDialogDescription>
				</AlertDialogHeader>
				<AlertDialogFooter>
					<AlertDialogCancel>
						{t("common.cancel")}
					</AlertDialogCancel>
					<AlertDialogAction onClick={onConfirm}>
						{t("common.delete")}
					</AlertDialogAction>
				</AlertDialogFooter>
			</AlertDialogContent>
		</AlertDialog>
	);
});
