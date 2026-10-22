import { createInstance } from "i18next";
import { createRoot } from "react-dom/client";
import { I18nextProvider } from "react-i18next";
import { ResourceMockup } from "./ResourceMockup";
import "./preview.css";
const i18n = createInstance();
void i18n.init({
	lng: "en",
	fallbackLng: "en",
	resources: {},
	interpolation: { escapeValue: false },
});
const root = document.getElementById("root");
if (root)
	createRoot(root).render(
		<I18nextProvider i18n={i18n}>
			<ResourceMockup />
		</I18nextProvider>,
	);
