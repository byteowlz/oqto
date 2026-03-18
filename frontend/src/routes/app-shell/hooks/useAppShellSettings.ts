import { useMountEffect } from "@/hooks/use-mount-effect";
import { setChatPrefetchLimit } from "@/lib/app-settings";
import { getSettingsValues } from "@/lib/control-plane-client";

export function useAppShellSettings(): void {
	useMountEffect(() => {
		let mounted = true;
		getSettingsValues("oqto")
			.then((values) => {
				if (!mounted) return;
				// Session limit unused but kept for future use
				setChatPrefetchLimit(values["sessions.chat_prefetch_limit"]?.value);
			})
			.catch(() => {
				setChatPrefetchLimit(null);
			});
		return () => {
			mounted = false;
		};
	});
}
