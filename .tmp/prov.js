import { createHotContext as __vite__createHotContext } from "/@vite/client";import.meta.hot = __vite__createHotContext("/components/providers.tsx");import __vite__cjsImport0_react_jsxDevRuntime from "/node_modules/.vite/deps/react_jsx-dev-runtime.js?v=92ce6bbd"; const jsxDEV = __vite__cjsImport0_react_jsxDevRuntime["jsxDEV"];
import { ThemeProvider } from "/components/theme-provider.tsx";
import { TooltipProvider } from "/components/ui/tooltip.tsx";
import { i18n } from "/lib/i18n.ts";
import { getQueryClient } from "/lib/query-client.ts";
import { QueryClientProvider } from "/node_modules/.vite/deps/@tanstack_react-query.js?v=92ce6bbd";
import { I18nextProvider } from "/node_modules/.vite/deps/react-i18next.js?v=92ce6bbd";
export function Providers({ children }) {
  const queryClient = getQueryClient();
  return /* @__PURE__ */ jsxDEV(QueryClientProvider, { client: queryClient, children: /* @__PURE__ */ jsxDEV(I18nextProvider, { i18n, children: /* @__PURE__ */ jsxDEV(
    ThemeProvider,
    {
      attribute: "class",
      defaultTheme: "dark",
      enableSystem: true,
      disableTransitionOnChange: true,
      children: /* @__PURE__ */ jsxDEV(TooltipProvider, { delayDuration: 0, children }, void 0, false, {
        fileName: "/home/wismut/byteowlz/oqto_refactor/frontend/components/providers.tsx",
        lineNumber: 25,
        columnNumber: 6
      }, this)
    },
    void 0,
    false,
    {
      fileName: "/home/wismut/byteowlz/oqto_refactor/frontend/components/providers.tsx",
      lineNumber: 19,
      columnNumber: 5
    },
    this
  ) }, void 0, false, {
    fileName: "/home/wismut/byteowlz/oqto_refactor/frontend/components/providers.tsx",
    lineNumber: 18,
    columnNumber: 4
  }, this) }, void 0, false, {
    fileName: "/home/wismut/byteowlz/oqto_refactor/frontend/components/providers.tsx",
    lineNumber: 17,
    columnNumber: 5
  }, this);
}
_c = Providers;
var _c;
$RefreshReg$(_c, "Providers");
import * as RefreshRuntime from "/@react-refresh";
const inWebWorker = typeof WorkerGlobalScope !== "undefined" && self instanceof WorkerGlobalScope;
if (import.meta.hot && !inWebWorker) {
  if (!window.$RefreshReg$) {
    throw new Error(
      "@vitejs/plugin-react can't detect preamble. Something is wrong."
    );
  }
  RefreshRuntime.__hmr_import(import.meta.url).then((currentExports) => {
    RefreshRuntime.registerExportsForReactRefresh("/home/wismut/byteowlz/oqto_refactor/frontend/components/providers.tsx", currentExports);
    import.meta.hot.accept((nextExports) => {
      if (!nextExports) return;
      const invalidateMessage = RefreshRuntime.validateRefreshBoundaryAndEnqueueUpdate("/home/wismut/byteowlz/oqto_refactor/frontend/components/providers.tsx", currentExports, nextExports);
      if (invalidateMessage) import.meta.hot.invalidate(invalidateMessage);
    });
  });
}
function $RefreshReg$(type, id) {
  return RefreshRuntime.register(type, "/home/wismut/byteowlz/oqto_refactor/frontend/components/providers.tsx " + id);
}
function $RefreshSig$() {
  return RefreshRuntime.createSignatureFunctionForTransform();
}

//# sourceMappingURL=data:application/json;base64,eyJ2ZXJzaW9uIjozLCJtYXBwaW5ncyI6IkFBd0JLO0FBeEJMLFNBQVNBLHFCQUFxQjtBQUM5QixTQUFTQyx1QkFBdUI7QUFDaEMsU0FBU0MsWUFBWTtBQUNyQixTQUFTQyxzQkFBc0I7QUFDL0IsU0FBU0MsMkJBQTJCO0FBRXBDLFNBQVNDLHVCQUF1QjtBQU16QixnQkFBU0MsVUFBVSxFQUFFQyxTQUF5QixHQUFHO0FBQ3ZELFFBQU1DLGNBQWNMLGVBQWU7QUFFbkMsU0FDQyx1QkFBQyx1QkFBb0IsUUFBUUssYUFDNUIsaUNBQUMsbUJBQWdCLE1BQ2hCO0FBQUEsSUFBQztBQUFBO0FBQUEsTUFDQSxXQUFVO0FBQUEsTUFDVixjQUFhO0FBQUEsTUFDYjtBQUFBLE1BQ0EsMkJBQXlCO0FBQUEsTUFFekIsaUNBQUMsbUJBQWdCLGVBQWUsR0FBSUQsWUFBcEM7QUFBQTtBQUFBO0FBQUE7QUFBQSxhQUE2QztBQUFBO0FBQUEsSUFOOUM7QUFBQTtBQUFBO0FBQUE7QUFBQTtBQUFBO0FBQUE7QUFBQTtBQUFBLEVBT0EsS0FSRDtBQUFBO0FBQUE7QUFBQTtBQUFBLFNBU0EsS0FWRDtBQUFBO0FBQUE7QUFBQTtBQUFBLFNBV0E7QUFFRjtBQUFDRSxLQWpCZUg7QUFBUyxJQUFBRztBQUFBQyxhQUFBRCxJQUFBIiwibmFtZXMiOlsiVGhlbWVQcm92aWRlciIsIlRvb2x0aXBQcm92aWRlciIsImkxOG4iLCJnZXRRdWVyeUNsaWVudCIsIlF1ZXJ5Q2xpZW50UHJvdmlkZXIiLCJJMThuZXh0UHJvdmlkZXIiLCJQcm92aWRlcnMiLCJjaGlsZHJlbiIsInF1ZXJ5Q2xpZW50IiwiX2MiLCIkUmVmcmVzaFJlZyQiXSwiaWdub3JlTGlzdCI6W10sInNvdXJjZXMiOlsicHJvdmlkZXJzLnRzeCJdLCJzb3VyY2VzQ29udGVudCI6WyJpbXBvcnQgeyBUaGVtZVByb3ZpZGVyIH0gZnJvbSBcIkAvY29tcG9uZW50cy90aGVtZS1wcm92aWRlclwiO1xuaW1wb3J0IHsgVG9vbHRpcFByb3ZpZGVyIH0gZnJvbSBcIkAvY29tcG9uZW50cy91aS90b29sdGlwXCI7XG5pbXBvcnQgeyBpMThuIH0gZnJvbSBcIkAvbGliL2kxOG5cIjtcbmltcG9ydCB7IGdldFF1ZXJ5Q2xpZW50IH0gZnJvbSBcIkAvbGliL3F1ZXJ5LWNsaWVudFwiO1xuaW1wb3J0IHsgUXVlcnlDbGllbnRQcm92aWRlciB9IGZyb20gXCJAdGFuc3RhY2svcmVhY3QtcXVlcnlcIjtcbmltcG9ydCB0eXBlIFJlYWN0IGZyb20gXCJyZWFjdFwiO1xuaW1wb3J0IHsgSTE4bmV4dFByb3ZpZGVyIH0gZnJvbSBcInJlYWN0LWkxOG5leHRcIjtcblxudHlwZSBQcm92aWRlcnNQcm9wcyA9IHtcblx0Y2hpbGRyZW46IFJlYWN0LlJlYWN0Tm9kZTtcbn07XG5cbmV4cG9ydCBmdW5jdGlvbiBQcm92aWRlcnMoeyBjaGlsZHJlbiB9OiBQcm92aWRlcnNQcm9wcykge1xuXHRjb25zdCBxdWVyeUNsaWVudCA9IGdldFF1ZXJ5Q2xpZW50KCk7XG5cblx0cmV0dXJuIChcblx0XHQ8UXVlcnlDbGllbnRQcm92aWRlciBjbGllbnQ9e3F1ZXJ5Q2xpZW50fT5cblx0XHRcdDxJMThuZXh0UHJvdmlkZXIgaTE4bj17aTE4bn0+XG5cdFx0XHRcdDxUaGVtZVByb3ZpZGVyXG5cdFx0XHRcdFx0YXR0cmlidXRlPVwiY2xhc3NcIlxuXHRcdFx0XHRcdGRlZmF1bHRUaGVtZT1cImRhcmtcIlxuXHRcdFx0XHRcdGVuYWJsZVN5c3RlbVxuXHRcdFx0XHRcdGRpc2FibGVUcmFuc2l0aW9uT25DaGFuZ2Vcblx0XHRcdFx0PlxuXHRcdFx0XHRcdDxUb29sdGlwUHJvdmlkZXIgZGVsYXlEdXJhdGlvbj17MH0+e2NoaWxkcmVufTwvVG9vbHRpcFByb3ZpZGVyPlxuXHRcdFx0XHQ8L1RoZW1lUHJvdmlkZXI+XG5cdFx0XHQ8L0kxOG5leHRQcm92aWRlcj5cblx0XHQ8L1F1ZXJ5Q2xpZW50UHJvdmlkZXI+XG5cdCk7XG59XG4iXSwiZmlsZSI6Ii9ob21lL3dpc211dC9ieXRlb3dsei9vcXRvX3JlZmFjdG9yL2Zyb250ZW5kL2NvbXBvbmVudHMvcHJvdmlkZXJzLnRzeCJ9