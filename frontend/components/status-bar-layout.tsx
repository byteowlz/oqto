import {
	Popover,
	PopoverContent,
	PopoverTrigger,
} from "@/components/ui/popover";
import { AlertCircle } from "lucide-react";
import { type ReactNode, useId } from "react";

/** Presentation only. The source retains notice lifetime and recovery policy. */
export type StatusBarNotice = {
	title: string;
	message: string;
	recovery?: { label: string; onActivate: () => void };
};

function NoticeDetails({ notice }: { notice: StatusBarNotice }) {
	const titleId = useId();
	const messageId = useId();
	return (
		<Popover>
			<PopoverTrigger asChild>
				<button
					type="button"
					className="flex h-full w-full min-w-0 items-center gap-1 text-destructive focus-visible:outline focus-visible:outline-2 focus-visible:outline-ring"
					aria-label={`${notice.title}: ${notice.message}`}
					title={notice.title}
				>
					<AlertCircle className="h-3 w-3 shrink-0" aria-hidden="true" />
					<span className="truncate">{notice.title}</span>
				</button>
			</PopoverTrigger>
			<PopoverContent
				side="top"
				align="center"
				className="max-h-[70dvh] w-80 max-w-[calc(100vw-2rem)] overflow-y-auto text-sm select-text"
				aria-labelledby={titleId}
				aria-describedby={messageId}
			>
				<h2 id={titleId} className="font-medium">
					{notice.title}
				</h2>
				<p id={messageId} className="mt-2 whitespace-pre-wrap break-words">
					{notice.message}
				</p>
				{notice.recovery ? (
					<button
						type="button"
						className="mt-3 min-h-11 text-primary underline underline-offset-4 focus-visible:outline focus-visible:outline-2 focus-visible:outline-ring"
						onClick={notice.recovery.onActivate}
					>
						{notice.recovery.label}
					</button>
				) : null}
			</PopoverContent>
		</Popover>
	);
}

/** One permanent lane; notice appearance and details never resize its siblings. */
export function StatusBarLayout({
	left,
	right,
	notice,
}: {
	left: ReactNode;
	right: ReactNode;
	notice?: StatusBarNotice | null;
}) {
	return (
		<div className="flex h-10 shrink-0 items-center gap-2 border-t border-sidebar-border bg-sidebar/80 px-4 text-[10px] text-muted-foreground select-none md:h-6 md:gap-3 md:px-3">
			<div className="flex min-w-0 max-w-[50%] items-center gap-2 md:gap-3">
				{left}
			</div>
			<div className="h-full min-w-0 flex-1 overflow-hidden">
				<output className="sr-only" aria-live="polite" aria-atomic="true">
					{notice ? `${notice.title}: ${notice.message}` : ""}
				</output>
				{notice ? <NoticeDetails notice={notice} /> : null}
			</div>
			<div className="flex shrink-0 items-center gap-2 md:gap-3">{right}</div>
		</div>
	);
}
