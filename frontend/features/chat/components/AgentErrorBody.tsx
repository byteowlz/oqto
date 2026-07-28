import { describeAgentError } from "@/features/chat/utils/agent-error";

/**
 * A failure explained in one line, with the captured output a click away.
 *
 * The runner sends the tail of the process's stderr, which is usually a stack
 * trace. Showing it raw buries the diagnosis; showing nothing is how these
 * failures used to reach people as an unrelated timeout. So lead with the
 * classified headline and keep the rest folded away.
 */
export function AgentErrorBody({ text }: { text: string }) {
	const { headline, cause, detail } = describeAgentError(text);
	// Repeating the headline under itself reads as an error about an error.
	const showCause = cause && cause !== headline;

	return (
		<div className="min-w-0 flex-1">
			<div className="font-medium">{headline}</div>

			{showCause && (
				<div className="mt-0.5 break-words font-mono text-xs opacity-70">
					{cause}
				</div>
			)}

			{detail && (
				<details className="group mt-1.5">
					<summary className="inline-flex cursor-pointer select-none items-center gap-1 text-xs opacity-60 hover:opacity-100 list-none [&::-webkit-details-marker]:hidden [&::marker]:content-['']">
						<svg
							className="h-3 w-3 shrink-0 transition-transform group-open:rotate-90"
							viewBox="0 0 24 24"
							fill="none"
							aria-hidden="true"
							stroke="currentColor"
							strokeWidth="2"
							strokeLinecap="round"
							strokeLinejoin="round"
						>
							<polyline points="9 18 15 12 9 6" />
						</svg>
						Details
					</summary>
					<pre className="mt-1.5 max-h-64 overflow-auto whitespace-pre-wrap break-words rounded bg-foreground/5 px-2 py-1.5 font-mono text-[11px] leading-relaxed opacity-80">
						{detail}
					</pre>
				</details>
			)}
		</div>
	);
}
