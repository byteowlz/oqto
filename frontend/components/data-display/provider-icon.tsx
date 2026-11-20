"use client";

import { cn } from "@/lib/utils";
import { Cpu } from "lucide-react";
import { providerIconUrl } from "./provider-icons/provider-icons";

type ProviderIconProps = {
	provider: string;
	className?: string;
};

/** Bundled monochrome silhouettes inherit the actual theme ink, not a dark-mode inversion. */
export function ProviderIcon({ provider, className }: ProviderIconProps) {
	const url = providerIconUrl(provider);
	const iconClass = cn("inline-block shrink-0 w-3 h-3", className);
	if (!url)
		return <Cpu role="img" aria-label={provider} className={iconClass} />;
	return (
		<span
			role="img"
			aria-label={provider}
			className={iconClass}
			style={{
				backgroundColor: "currentColor",
				maskImage: `url("${url}")`,
				WebkitMaskImage: `url("${url}")`,
				maskSize: "contain",
				WebkitMaskSize: "contain",
				maskPosition: "center",
				WebkitMaskPosition: "center",
				maskRepeat: "no-repeat",
				WebkitMaskRepeat: "no-repeat",
			}}
		/>
	);
}
