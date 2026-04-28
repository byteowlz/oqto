import type { VisualRuntimeCapability, VisualRuntimeMode } from "./types";

export const VISUAL_RUNTIME_MODE_DEFAULT: VisualRuntimeMode = "offline_strict";

export const LOCAL_ASSET_PATHS: Record<VisualRuntimeCapability, string> = {
	morphdom: "/visual-runtime/vendor/morphdom-umd.min.js",
	mermaid: "/visual-runtime/vendor/mermaid.min.js",
	layout_elk: "/visual-runtime/vendor/mermaid-layout-elk.esm.min.mjs",
	chartjs: "/visual-runtime/vendor/chart.umd.min.js",
	three: "/visual-runtime/vendor/three.min.js",
};

export const KNOWN_REMOTE_ASSET_PATTERNS: Array<{
	capability: VisualRuntimeCapability;
	pattern: RegExp;
}> = [
	{
		capability: "mermaid",
		pattern:
			/^https?:\/\/(?:cdn\.jsdelivr\.net|unpkg\.com|esm\.sh)\/(?:npm\/)?mermaid@?/i,
	},
	{
		capability: "layout_elk",
		pattern:
			/^https?:\/\/(?:cdn\.jsdelivr\.net|unpkg\.com|esm\.sh)\/(?:npm\/)?@mermaid-js\/layout-elk@?/i,
	},
	{
		capability: "chartjs",
		pattern:
			/^https?:\/\/(?:cdn\.jsdelivr\.net|unpkg\.com|esm\.sh)\/(?:npm\/)?chart\.js@?/i,
	},
	{
		capability: "morphdom",
		pattern:
			/^https?:\/\/(?:cdn\.jsdelivr\.net|unpkg\.com|esm\.sh)\/(?:npm\/)?morphdom@?/i,
	},
	{
		capability: "three",
		pattern:
			/^https?:\/\/(?:cdn\.jsdelivr\.net|unpkg\.com|esm\.sh)\/(?:npm\/)?three@?/i,
	},
];

export function cspForMode(mode: VisualRuntimeMode, nonce?: string): string {
	const nonceToken = nonce ? `'nonce-${nonce}'` : "'unsafe-inline'";

	if (mode === "online_flexible") {
		return [
			"default-src 'self' data: blob: https: http:",
			`script-src 'self' ${nonceToken} 'unsafe-eval' https: http:`,
			`style-src 'self' ${nonceToken} https: http:`,
			"img-src 'self' data: blob: https: http:",
			"font-src 'self' data: https: http:",
			"connect-src 'self' https: http: ws: wss:",
			"frame-ancestors 'self'",
		].join("; ");
	}

	if (mode === "offline_prefer") {
		return [
			"default-src 'self' data: blob:",
			`script-src 'self' ${nonceToken}`,
			`style-src 'self' ${nonceToken}`,
			"img-src 'self' data: blob:",
			"font-src 'self' data:",
			"connect-src 'self'",
			"frame-ancestors 'self'",
		].join("; ");
	}

	return [
		"default-src 'none'",
		`script-src 'self' ${nonceToken}`,
		`style-src 'self' ${nonceToken}`,
		"img-src 'self' data: blob:",
		"font-src 'self' data:",
		"connect-src 'none'",
		"frame-ancestors 'self'",
	].join("; ");
}

export function buildVisualRuntimeBootstrap(
	capabilities: VisualRuntimeCapability[],
): string {
	const has = new Set(capabilities);
	return `<script>
(function() {
  var listeners = [];
  function getTheme() {
    if (window.apphost && typeof window.apphost.theme === 'string') return window.apphost.theme;
    return 'dark';
  }

  var runtime = window.VisualRuntime || {
    libs: {},
    theme: {
      get: getTheme,
      subscribe: function(cb) {
        listeners.push(cb);
        if (window.apphost && typeof window.apphost.onThemeChange === 'function') {
          return window.apphost.onThemeChange(cb);
        }
        return function() {
          listeners = listeners.filter(function(x) { return x !== cb; });
        };
      }
    },
    events: {
      emit: function(type, payload) {
        if (window.apphost && typeof window.apphost.send === 'function') {
          window.apphost.send({ type: type, payload: payload });
        }
      }
    }
  };

  runtime.libs.mermaid = window.mermaid || runtime.libs.mermaid || null;
  runtime.libs.Chart = window.Chart || runtime.libs.Chart || null;
  runtime.libs.morphdom = window.morphdom || runtime.libs.morphdom || null;
  runtime.libs.THREE = window.THREE || runtime.libs.THREE || null;
  runtime.libs.layoutELK = window.mermaidLayoutELK || runtime.libs.layoutELK || null;

  window.VisualRuntime = runtime;

  ${has.has("mermaid") ? "if (!runtime.libs.mermaid) console.warn('[VisualRuntime] mermaid requested but not available locally.');" : ""}
  ${has.has("chartjs") ? "if (!runtime.libs.Chart) console.warn('[VisualRuntime] chart.js requested but not available locally.');" : ""}
  ${has.has("morphdom") ? "if (!runtime.libs.morphdom) console.warn('[VisualRuntime] morphdom requested but not available locally.');" : ""}
})();
</script>`;
}
