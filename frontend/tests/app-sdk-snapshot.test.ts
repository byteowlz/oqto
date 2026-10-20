import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { OQTO_APP_PROTOCOL_V3 } from "@byteowlz/oqto-app-sdk";
import sdk from "@byteowlz/oqto-app-sdk/package.json";
import { expect, it } from "vitest";

it("uses the portable, hash-pinned canonical SDK distribution, not a sibling checkout", () => {
	const provenance = JSON.parse(
		readFileSync(
			resolve(process.cwd(), "vendor/oqto-app-sdk/PROVENANCE.json"),
			"utf8",
		),
	);
	const manifest = JSON.parse(
		readFileSync(resolve(process.cwd(), "package.json"), "utf8"),
	);
	const spec = `file:vendor/oqto-app-sdk/byteowlz-oqto-app-sdk-${provenance.version}.tgz`;
	expect(manifest.dependencies["@byteowlz/oqto-app-sdk"]).toBe(spec);
	expect(sdk.version).toBe(provenance.version);
	expect(provenance.source_commit).toMatch(/^[a-f0-9]{40}$/);
	const bytes = readFileSync(resolve(process.cwd(), spec.slice(5)));
	expect(createHash("sha256").update(bytes).digest("hex")).toBe(
		provenance.tarball_sha256,
	);
	expect(OQTO_APP_PROTOCOL_V3).toBe("oqto-app/v3");
});
