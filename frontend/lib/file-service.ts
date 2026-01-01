import { appConfig } from "@/lib/config";

export type FileNode = {
	name: string;
	path: string;
	type: "file" | "directory";
	children?: FileNode[];
};

export async function fetchFileTree(path = "."): Promise<FileNode[]> {
	if (!appConfig.fileServerBaseUrl) {
		throw new Error("VITE_FILE_SERVER_URL is not configured");
	}
	const url = new URL("/tree", `${appConfig.fileServerBaseUrl}/`);
	url.searchParams.set("path", path);
	const res = await fetch(url.toString(), { cache: "no-store" });
	if (!res.ok) {
		const text = await res.text().catch(() => res.statusText);
		throw new Error(text || `File server error (${res.status})`);
	}
	return res.json();
}

export async function fetchFileContent(path: string): Promise<string> {
	if (!appConfig.fileServerBaseUrl) {
		throw new Error("VITE_FILE_SERVER_URL is not configured");
	}
	const url = new URL("/file", `${appConfig.fileServerBaseUrl}/`);
	url.searchParams.set("path", path);
	const res = await fetch(url.toString(), { cache: "no-store" });
	if (!res.ok) {
		const text = await res.text().catch(() => res.statusText);
		throw new Error(text || `Unable to fetch ${path}`);
	}
	return res.text();
}
