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

export interface RenameSessionDialogProps {
	open: boolean;
	onOpenChange: (open: boolean) => void;
	initialValue: string;
	onConfirm: (newValue: string) => void;
	locale: string;
}

export const RenameSessionDialog = memo(function RenameSessionDialog({
	open,
	onOpenChange,
	initialValue,
	onConfirm,
	locale,
}: RenameSessionDialogProps) {
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
						{t("dialogs.renameChat")}
					</DialogTitle>
					<DialogDescription>
						{t("dialogs.renameChatDescription")}
					</DialogDescription>
				</DialogHeader>
				<Input
					value={value}
					onChange={(e) => setValue(e.target.value)}
					placeholder={t("dialogs.chatTitle")}
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
