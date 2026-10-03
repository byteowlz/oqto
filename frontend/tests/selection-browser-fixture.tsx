// Disposable file:// browser proof, not an authenticated deployment claim.
import { createInstance } from "i18next";
import { createRoot } from "react-dom/client";
import { I18nextProvider } from "react-i18next";
import en from "../messages/en.json";
import { SelectionView } from "../src/oqto-ui/chat/SelectionView";
import { review, selectionHarness, settle } from "./selection-fixtures";

async function main() {
	const i18n = createInstance();
	await i18n.init({
		lng: "en",
		resources: { en: { translation: en } },
		interpolation: { escapeValue: false },
	});
	const questions = selectionHarness();
	const inventory = selectionHarness(review("review"));
	const element = document.getElementById("root");
	if (!element) throw new Error("Missing fixture root");
	createRoot(element).render(
		<I18nextProvider i18n={i18n}>
			<SelectionView
				host={{ session: () => questions.controller }}
				sessionId="questions"
			/>
			<SelectionView
				host={{ session: () => inventory.controller }}
				sessionId="review"
			/>
		</I18nextProvider>,
	);
	const stopQuestions = questions.controller.subscribe(() => {});
	const stopInventory = inventory.controller.subscribe(() => {});
	await settle();
	questions.present();
	inventory.present();
	window.addEventListener(
		"pagehide",
		() => {
			stopQuestions();
			stopInventory();
		},
		{ once: true },
	);
}
void main();
