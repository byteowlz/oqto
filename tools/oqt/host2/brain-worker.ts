/** host2 coding brain entry. Runs only inside the oqt_brain sandbox, launched
 * by the guard (guard-launcher.ts): the shared gated composition, served to
 * the server as `session:<id>`. Holds no authority (see coding-composition.ts).
 */
import { BACKGROUND_CONTEXT } from "@earendil-works/chord/context";
import { openBrainComposition } from "../experimental/coding-composition.ts";
import { runWorker } from "./worker.ts";

await runWorker(async (init) => {
	if (!init.cwd) throw new Error("Coding brain requires its workspace");
	const composition = await openBrainComposition({
		databasePath: init.database,
		session: init.session,
		cwd: init.cwd,
	});
	const { harness, conversation } = composition.runtime;
	return {
		harness,
		root: conversation,
		async close() {
			try {
				await harness.close(BACKGROUND_CONTEXT);
			} finally {
				composition.close();
			}
		},
	};
});
