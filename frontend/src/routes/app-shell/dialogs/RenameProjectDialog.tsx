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
import { useTranslation } from "react-i18next";

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
	const { t } = useTranslation();
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
						{t("projects.renameProject")}
					</DialogTitle>
					<DialogDescription>
						{t("projects.renameDescription")}
					</DialogDescription>
				</DialogHeader>
				<Input
					value={value}
					onChange={(e) => setValue(e.target.value)}
					placeholder={t("projects.projectName")}
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
						{t("common.cancel")}
					</Button>
					<Button type="button" onClick={handleConfirm}>
						{t("common.save")}
					</Button>
				</DialogFooter>
			</DialogContent>
		</Dialog>
	);
});
