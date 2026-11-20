// Isolated production status-bar render with synthetic surrounding content.
// Not a full shell, authenticated Runner, native render or acceptance claim.
import { StatusBarLayout } from "@/components/status-bar-layout";
import { useState } from "react";
import { createRoot } from "react-dom/client";
import "../src/styles/globals.css";

const sessionIds = Array.from({ length: 50 }, (_, index) => `fixture-${index}`);

function Fixture() {
	const [failed, setFailed] = useState(false);
	const [retries, setRetries] = useState(0);
	return (
		<div className="flex h-dvh flex-col bg-background text-foreground">
			<div className="flex shrink-0 gap-3 p-2 text-xs">
				<button type="button" onClick={() => setFailed((value) => !value)}>
					Toggle catalog error
				</button>
				<output aria-label="Recovery calls">{retries}</output>
			</div>
			<div className="flex min-h-0 flex-1">
				<aside
					data-proof="list"
					className="w-1/3 overflow-auto border-r border-border p-2 text-xs"
				>
					{sessionIds.map((id) => (
						<p key={id} className="py-2">
							Synthetic Session {id}
						</p>
					))}
				</aside>
				<main className="flex min-w-0 flex-1 flex-col">
					<div
						data-proof="transcript"
						className="min-h-0 flex-1 overflow-auto p-3"
					>
						Synthetic transcript. Error details must not move this region.
					</div>
					<textarea
						data-proof="composer"
						aria-label="Retained draft"
						defaultValue="Unsent fixture draft"
						className="m-2 shrink-0 border border-border p-2"
					/>
					<StatusBarLayout
						left={
							<span data-proof="model" className="truncate">
								Model · thinking
							</span>
						}
						right={<span data-proof="version">v0.5.11</span>}
						notice={
							failed
								? {
										title: "Chat-Verlauf nicht erreichbar",
										message: `Synthetische Diagnose: ${"Verbindungsunterbrechung".repeat(12)}\nDer Entwurf bleibt erhalten.`,
										recovery: {
											label: "Erneut versuchen",
											onActivate: () => setRetries((value) => value + 1),
										},
									}
								: null
						}
					/>
				</main>
			</div>
		</div>
	);
}

const root = document.getElementById("root");
if (!root) throw new Error("Missing synthetic fixture root");
createRoot(root).render(<Fixture />);
