import { Providers } from "@/components/providers";
import { initI18n } from "@/lib/i18n";
import React from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App";
import "./styles/globals.css";

initI18n();

const container = document.getElementById("root");
if (!container) {
	throw new Error("Root container missing");
}

createRoot(container).render(
	<React.StrictMode>
		<Providers>
			<App />
		</Providers>
	</React.StrictMode>,
);
