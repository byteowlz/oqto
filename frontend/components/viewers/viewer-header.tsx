import type { ReactNode } from "react";

interface ViewerHeaderProps {
	icon: ReactNode;
	filename?: string;
	metadata?: ReactNode;
	actions?: ReactNode;
}

/** Shared file heading; viewing/copy state stays with the owning renderer. */
export function ViewerHeader({
	icon,
	filename,
	metadata,
	actions,
}: ViewerHeaderProps) {
	return (
		<div className="flex items-center justify-between px-3 py-2 bg-muted border-b border-border shrink-0">
			<div className="flex items-center gap-2">
				{icon}
				{filename && (
					<span className="text-sm font-medium truncate max-w-[200px]">
						{filename}
					</span>
				)}
				{metadata}
			</div>
			{actions}
		</div>
	);
}
