"use client";

import { cn } from "@/lib/utils";
import { Cpu } from "lucide-react";
import { useState } from "react";

type ProviderIconProps = {
	provider: string;
	className?: string;
};

// Base URL for provider logos from models.dev repo
const LOGO_BASE_URL = "https://raw.githubusercontent.com/anomalyco/models.dev/dev/providers";

// Known providers in models.dev (use folder name as key)
const PROVIDER_FOLDER_MAP: Record<string, string> = {
	// Direct matches
	anthropic: "anthropic",
	openai: "openai",
	google: "google",
	mistral: "mistral",
	cohere: "cohere",
	deepseek: "deepseek",
	xai: "xai",
	groq: "groq",
	together: "together-ai",
	fireworks: "fireworks-ai",
	perplexity: "perplexity",
	replicate: "replicate",
	anyscale: "anyscale",
	// Aliases
	bedrock: "amazon-bedrock",
	"amazon-bedrock": "amazon-bedrock",
	aws: "amazon-bedrock",
	azure: "azure",
	"azure-openai": "azure",
	gemini: "google",
	grok: "xai",
	"together-ai": "together-ai",
	"fireworks-ai": "fireworks-ai",
	ollama: "ollama",
	openrouter: "openrouter",
	cerebras: "cerebras",
	sambanova: "sambanova",
	hyperbolic: "hyperbolic",
	lambda: "lambda",
	lepton: "lepton",
	novita: "novita",
	deepinfra: "deepinfra",
	alibaba: "alibaba",
	cloudflare: "cloudflare-workers-ai",
};

function getLogoUrl(provider: string): string | null {
	const normalized = provider.toLowerCase();
	const folder = PROVIDER_FOLDER_MAP[normalized];
	if (folder) {
		return `${LOGO_BASE_URL}/${folder}/logo.svg`;
	}
	return null;
}

export function ProviderIcon({ provider, className }: ProviderIconProps) {
	const [hasError, setHasError] = useState(false);
	const logoUrl = getLogoUrl(provider);
	const iconClass = cn("w-3 h-3", className);

	// Use logo from models.dev if available
	// SVGs use currentColor, so we apply CSS filter to invert in dark mode
	if (logoUrl && !hasError) {
		return (
			<img
				src={logoUrl}
				alt={provider}
				className={cn(iconClass, "dark:invert")}
				onError={() => setHasError(true)}
			/>
		);
	}

	// Fallback: generic CPU icon for custom/unknown providers
	return <Cpu className={iconClass} />;
}
