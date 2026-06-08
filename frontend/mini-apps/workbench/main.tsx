import "@/src/styles/globals.css";
import React from "react";
import { createRoot } from "react-dom/client";
import { Workbench } from "./Workbench";

const container = document.getElementById("root");
if (!container) {
	throw new Error("Root container missing");
}

createRoot(container).render(
	<React.StrictMode>
		<Workbench />
	</React.StrictMode>,
);
