import { ProviderIcon } from "@/components/data-display/provider-icon";
import catalog from "@/components/data-display/provider-icons/provider-icons-catalog.json";
import { useState } from "react";
import { createRoot } from "react-dom/client";
import "./provider-icons.css";

/** Synthetic component/tint verification, not a live authenticated Session or selected layout. */
function Fixture() {
	const [provider, setProvider] = useState("custom-provider");
	return (
		<main>
			<h1>Provider icon component fixture</h1>
			<button type="button" onClick={() => setProvider("openai")}>
				<ProviderIcon provider={provider} /> Change the synthetic provider
			</button>
			{["dark", "chalk", "tint"].map((theme) => (
				<section key={theme} data-theme={theme}>
					<h2>{theme}</h2>
					<div className="provider-grid">
						{Object.keys(catalog.icons).map((brand) => (
							<div key={brand} className="provider-cell">
								<ProviderIcon provider={brand} />
								<span>{brand}</span>
							</div>
						))}
					</div>
				</section>
			))}
		</main>
	);
}

const root = document.getElementById("root");
if (!root) throw new Error("Missing synthetic fixture root");
createRoot(root).render(<Fixture />);
