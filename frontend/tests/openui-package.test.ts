import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

const provenance = JSON.parse(
	readFileSync(
		resolve(process.cwd(), "vendor/oqto-openui/PROVENANCE.json"),
		"utf8",
	),
);

describe("vendored OpenUI build input", () => {
	it("ships the reviewed archive with its recorded checksum", () => {
		const archive = readFileSync(
			resolve(process.cwd(), `vendor/oqto-openui/${provenance.tarball}`),
		);
		expect(createHash("sha256").update(archive).digest("hex")).toBe(
			provenance.sha256,
		);
	});

	it("uses that archive rather than a neighbouring development checkout", () => {
		const pkg = JSON.parse(
			readFileSync(resolve(process.cwd(), "package.json"), "utf8"),
		);
		expect(pkg.dependencies["@byteowlz/oqto-openui"]).toBe(
			`file:vendor/oqto-openui/${provenance.tarball}`,
		);
	});
});
