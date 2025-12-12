#!/usr/bin/env node

import { spawn, execSync } from "node:child_process";
import { join } from "node:path";
import { homedir, platform } from "node:os";
import { existsSync, mkdirSync, cpSync } from "node:fs";
import puppeteer from "puppeteer-core";

const useProfile = process.argv[2] === "--profile";

if (process.argv[2] && process.argv[2] !== "--profile") {
	console.log("Usage: start.ts [--profile]");
	console.log("\nOptions:");
	console.log("  --profile  Copy your default Chrome profile (cookies, logins)");
	console.log("\nExamples:");
	console.log("  start.ts            # Start with fresh profile");
	console.log("  start.ts --profile  # Start with your Chrome profile");
	process.exit(1);
}

const os = platform();
const home = homedir();

// Get platform-specific Chrome paths
function getChromeConfig() {
	switch (os) {
		case "darwin":
			return {
				executable: "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
				profilePath: join(home, "Library/Application Support/Google/Chrome"),
				killCommand: "killall 'Google Chrome'",
			};
		case "linux":
			return {
				executable: "google-chrome",
				profilePath: join(home, ".config/google-chrome"),
				killCommand: "killall chrome",
			};
		case "win32":
			return {
				executable: join(process.env["PROGRAMFILES"] || "C:\\Program Files", "Google\\Chrome\\Application\\chrome.exe"),
				profilePath: join(home, "AppData\\Local\\Google\\Chrome\\User Data"),
				killCommand: "taskkill /F /IM chrome.exe",
			};
		default:
			throw new Error(`Unsupported platform: ${os}`);
	}
}

const chromeConfig = getChromeConfig();
const cacheDir = join(home, ".cache", "scraping");

// Kill existing Chrome
try {
	execSync(chromeConfig.killCommand, { stdio: "ignore" });
} catch {}

// Wait a bit for processes to fully die
await new Promise((r) => setTimeout(r, 1000));

// Setup profile directory
if (!existsSync(cacheDir)) {
	mkdirSync(cacheDir, { recursive: true });
}

if (useProfile) {
	// Copy profile using Node.js API for cross-platform compatibility
	if (existsSync(chromeConfig.profilePath)) {
		try {
			cpSync(chromeConfig.profilePath, cacheDir, { recursive: true });
		} catch (err) {
			console.error(`✗ Failed to copy profile: ${err.message}`);
			process.exit(1);
		}
	} else {
		console.error(`✗ Chrome profile not found at: ${chromeConfig.profilePath}`);
		process.exit(1);
	}
}

// Start Chrome in background (detached so Node can exit)
if (!existsSync(chromeConfig.executable) && os !== "linux") {
	console.error(`✗ Chrome not found at: ${chromeConfig.executable}`);
	process.exit(1);
}

spawn(
	chromeConfig.executable,
	["--remote-debugging-port=9222", `--user-data-dir=${cacheDir}`],
	{ detached: true, stdio: "ignore" },
).unref();

// Wait for Chrome to be ready by attempting to connect
let connected = false;
for (let i = 0; i < 30; i++) {
	try {
		const browser = await puppeteer.connect({
			browserURL: "http://localhost:9222",
			defaultViewport: null,
		});
		await browser.disconnect();
		connected = true;
		break;
	} catch {
		await new Promise((r) => setTimeout(r, 500));
	}
}

if (!connected) {
	console.error("✗ Failed to connect to Chrome");
	process.exit(1);
}

console.log(`✓ Chrome started on :9222${useProfile ? " with your profile" : ""}`);
