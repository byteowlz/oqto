import { fileURLToPath } from "node:url";
import { stageMoonshineRuntime } from "@byteowlz/ears-browser/stage-runtime";

await stageMoonshineRuntime(
	fileURLToPath(new URL("../public/speech/moonshine-0.1.5/", import.meta.url)),
);
