import { controlPlaneApiUrl, getAuthHeaders } from "@/lib/control-plane-client";

export type SlideMetadata = {
	title?: string;
	description?: string;
	topic?: string;
	tags?: string[];
	template?: string;
	layout?: string;
	research_area?: string;
	author?: string;
	created?: string;
	modified?: string;
};

export type SlideSummary = {
	name: string;
	relative_path: string;
	metadata: SlideMetadata;
};

export type SlidevConfig = {
	theme?: string;
	drawings?: boolean;
	transition?: string;
	title?: string;
	dark_mode?: boolean;
	aspect_ratio?: string;
	canvas_width?: number;
	record?: boolean;
};

export type Skeleton = {
	name: string;
	title?: string;
	description?: string;
	slides: string[];
	flavor?: string;
	slidev_config: SlidevConfig;
};

export type Flavor = {
	name: string;
	display_name?: string;
	description?: string;
	colors?: {
		primary?: string;
		secondary?: string;
		background?: string;
		text?: string;
		accent?: string;
		code_background?: string;
	};
	typography?: {
		heading_font?: string;
		body_font?: string;
		code_font?: string;
		base_size?: string;
	};
	background?: {
		background_type?: string;
		value?: string;
		opacity?: number;
	};
	assets_dir?: string | null;
};

export type PreviewResponse = {
	session_id: string;
	url: string;
	port: number;
};

const defaultHeaders = () => ({
	...getAuthHeaders(),
});

async function readJson<T>(res: Response): Promise<T> {
	if (!res.ok) {
		const text = await res.text();
		throw new Error(text || `Request failed (${res.status})`);
	}
	return (await res.json()) as T;
}

export async function listSlides(): Promise<SlideSummary[]> {
	const res = await fetch(controlPlaneApiUrl("/api/sldr/slides"), {
		headers: defaultHeaders(),
		credentials: "include",
	});
	const data = await readJson<{ slides: SlideSummary[] }>(res);
	return data.slides;
}

export async function listSkeletons(): Promise<Skeleton[]> {
	const res = await fetch(controlPlaneApiUrl("/api/sldr/skeletons"), {
		headers: defaultHeaders(),
		credentials: "include",
	});
	const data = await readJson<{ skeletons: Skeleton[] }>(res);
	return data.skeletons;
}

export async function listFlavors(): Promise<Flavor[]> {
	const res = await fetch(controlPlaneApiUrl("/api/sldr/flavors"), {
		headers: defaultHeaders(),
		credentials: "include",
	});
	const data = await readJson<{ flavors: Flavor[] }>(res);
	return data.flavors;
}

export async function previewSkeleton(
	skeleton: string,
	flavor?: string,
): Promise<PreviewResponse> {
	const params = new URLSearchParams();
	if (flavor) params.set("flavor", flavor);
	const res = await fetch(
		controlPlaneApiUrl(`/api/sldr/preview/${encodeURIComponent(skeleton)}?${params.toString()}`),
		{
			headers: defaultHeaders(),
			credentials: "include",
		},
	);
	return readJson<PreviewResponse>(res);
}
