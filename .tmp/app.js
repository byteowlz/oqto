import { createHotContext as __vite__createHotContext } from "/@vite/client";import.meta.hot = __vite__createHotContext("/src/App.tsx");import __vite__cjsImport0_react_jsxDevRuntime from "/node_modules/.vite/deps/react_jsx-dev-runtime.js?v=92ce6bbd"; const jsxDEV = __vite__cjsImport0_react_jsxDevRuntime["jsxDEV"];
import { BrowserRouter, Route, Routes } from "/node_modules/.vite/deps/react-router-dom.js?v=92ce6bbd";
import { AppShellRoute } from "/src/routes/AppShellRoute.tsx?t=1776551760687";
import { AuthLayout } from "/src/routes/AuthLayout.tsx";
import { LoginPage } from "/src/routes/LoginPage.tsx?t=1775370528558";
import { RegisterPage } from "/src/routes/RegisterPage.tsx?t=1775370528558";
import { RequireAuth } from "/src/routes/RequireAuth.tsx?t=1775370528558";
export function App() {
  return /* @__PURE__ */ jsxDEV(
    BrowserRouter,
    {
      future: {
        v7_startTransition: true,
        v7_relativeSplatPath: true
      },
      children: /* @__PURE__ */ jsxDEV(Routes, { children: [
        /* @__PURE__ */ jsxDEV(
          Route,
          {
            path: "/login",
            element: /* @__PURE__ */ jsxDEV(AuthLayout, { children: /* @__PURE__ */ jsxDEV(LoginPage, {}, void 0, false, {
              fileName: "/home/wismut/byteowlz/oqto_refactor/frontend/src/App.tsx",
              lineNumber: 21,
              columnNumber: 8
            }, this) }, void 0, false, {
              fileName: "/home/wismut/byteowlz/oqto_refactor/frontend/src/App.tsx",
              lineNumber: 20,
              columnNumber: 11
            }, this)
          },
          void 0,
          false,
          {
            fileName: "/home/wismut/byteowlz/oqto_refactor/frontend/src/App.tsx",
            lineNumber: 17,
            columnNumber: 5
          },
          this
        ),
        /* @__PURE__ */ jsxDEV(
          Route,
          {
            path: "/register",
            element: /* @__PURE__ */ jsxDEV(AuthLayout, { children: /* @__PURE__ */ jsxDEV(RegisterPage, {}, void 0, false, {
              fileName: "/home/wismut/byteowlz/oqto_refactor/frontend/src/App.tsx",
              lineNumber: 29,
              columnNumber: 8
            }, this) }, void 0, false, {
              fileName: "/home/wismut/byteowlz/oqto_refactor/frontend/src/App.tsx",
              lineNumber: 28,
              columnNumber: 11
            }, this)
          },
          void 0,
          false,
          {
            fileName: "/home/wismut/byteowlz/oqto_refactor/frontend/src/App.tsx",
            lineNumber: 25,
            columnNumber: 5
          },
          this
        ),
        /* @__PURE__ */ jsxDEV(
          Route,
          {
            path: "/*",
            element: /* @__PURE__ */ jsxDEV(RequireAuth, { children: /* @__PURE__ */ jsxDEV(AppShellRoute, {}, void 0, false, {
              fileName: "/home/wismut/byteowlz/oqto_refactor/frontend/src/App.tsx",
              lineNumber: 37,
              columnNumber: 8
            }, this) }, void 0, false, {
              fileName: "/home/wismut/byteowlz/oqto_refactor/frontend/src/App.tsx",
              lineNumber: 36,
              columnNumber: 11
            }, this)
          },
          void 0,
          false,
          {
            fileName: "/home/wismut/byteowlz/oqto_refactor/frontend/src/App.tsx",
            lineNumber: 33,
            columnNumber: 5
          },
          this
        )
      ] }, void 0, true, {
        fileName: "/home/wismut/byteowlz/oqto_refactor/frontend/src/App.tsx",
        lineNumber: 16,
        columnNumber: 4
      }, this)
    },
    void 0,
    false,
    {
      fileName: "/home/wismut/byteowlz/oqto_refactor/frontend/src/App.tsx",
      lineNumber: 10,
      columnNumber: 5
    },
    this
  );
}
_c = App;
var _c;
$RefreshReg$(_c, "App");
import * as RefreshRuntime from "/@react-refresh";
const inWebWorker = typeof WorkerGlobalScope !== "undefined" && self instanceof WorkerGlobalScope;
if (import.meta.hot && !inWebWorker) {
  if (!window.$RefreshReg$) {
    throw new Error(
      "@vitejs/plugin-react can't detect preamble. Something is wrong."
    );
  }
  RefreshRuntime.__hmr_import(import.meta.url).then((currentExports) => {
    RefreshRuntime.registerExportsForReactRefresh("/home/wismut/byteowlz/oqto_refactor/frontend/src/App.tsx", currentExports);
    import.meta.hot.accept((nextExports) => {
      if (!nextExports) return;
      const invalidateMessage = RefreshRuntime.validateRefreshBoundaryAndEnqueueUpdate("/home/wismut/byteowlz/oqto_refactor/frontend/src/App.tsx", currentExports, nextExports);
      if (invalidateMessage) import.meta.hot.invalidate(invalidateMessage);
    });
  });
}
function $RefreshReg$(type, id) {
  return RefreshRuntime.register(type, "/home/wismut/byteowlz/oqto_refactor/frontend/src/App.tsx " + id);
}
function $RefreshSig$() {
  return RefreshRuntime.createSignatureFunctionForTransform();
}

//# sourceMappingURL=data:application/json;base64,eyJ2ZXJzaW9uIjozLCJtYXBwaW5ncyI6IkFBb0JPO0FBcEJQLFNBQVNBLGVBQWVDLE9BQU9DLGNBQWM7QUFDN0MsU0FBU0MscUJBQXFCO0FBQzlCLFNBQVNDLGtCQUFrQjtBQUMzQixTQUFTQyxpQkFBaUI7QUFDMUIsU0FBU0Msb0JBQW9CO0FBQzdCLFNBQVNDLG1CQUFtQjtBQUVyQixnQkFBU0MsTUFBTTtBQUNyQixTQUNDO0FBQUEsSUFBQztBQUFBO0FBQUEsTUFDQSxRQUFRO0FBQUEsUUFDUEMsb0JBQW9CO0FBQUEsUUFDcEJDLHNCQUFzQjtBQUFBLE1BQ3ZCO0FBQUEsTUFFQSxpQ0FBQyxVQUNBO0FBQUE7QUFBQSxVQUFDO0FBQUE7QUFBQSxZQUNBLE1BQUs7QUFBQSxZQUNMLFNBQ0MsdUJBQUMsY0FDQSxpQ0FBQyxlQUFEO0FBQUE7QUFBQTtBQUFBO0FBQUEsbUJBQVUsS0FEWDtBQUFBO0FBQUE7QUFBQTtBQUFBLG1CQUVBO0FBQUE7QUFBQSxVQUxGO0FBQUE7QUFBQTtBQUFBO0FBQUE7QUFBQTtBQUFBO0FBQUE7QUFBQSxRQU1FO0FBQUEsUUFFRjtBQUFBLFVBQUM7QUFBQTtBQUFBLFlBQ0EsTUFBSztBQUFBLFlBQ0wsU0FDQyx1QkFBQyxjQUNBLGlDQUFDLGtCQUFEO0FBQUE7QUFBQTtBQUFBO0FBQUEsbUJBQWEsS0FEZDtBQUFBO0FBQUE7QUFBQTtBQUFBLG1CQUVBO0FBQUE7QUFBQSxVQUxGO0FBQUE7QUFBQTtBQUFBO0FBQUE7QUFBQTtBQUFBO0FBQUE7QUFBQSxRQU1FO0FBQUEsUUFFRjtBQUFBLFVBQUM7QUFBQTtBQUFBLFlBQ0EsTUFBSztBQUFBLFlBQ0wsU0FDQyx1QkFBQyxlQUNBLGlDQUFDLG1CQUFEO0FBQUE7QUFBQTtBQUFBO0FBQUEsbUJBQWMsS0FEZjtBQUFBO0FBQUE7QUFBQTtBQUFBLG1CQUVBO0FBQUE7QUFBQSxVQUxGO0FBQUE7QUFBQTtBQUFBO0FBQUE7QUFBQTtBQUFBO0FBQUE7QUFBQSxRQU1FO0FBQUEsV0F2Qkg7QUFBQTtBQUFBO0FBQUE7QUFBQSxhQXlCQTtBQUFBO0FBQUEsSUEvQkQ7QUFBQTtBQUFBO0FBQUE7QUFBQTtBQUFBO0FBQUE7QUFBQTtBQUFBLEVBZ0NBO0FBRUY7QUFBQ0MsS0FwQ2VIO0FBQUcsSUFBQUc7QUFBQUMsYUFBQUQsSUFBQSIsIm5hbWVzIjpbIkJyb3dzZXJSb3V0ZXIiLCJSb3V0ZSIsIlJvdXRlcyIsIkFwcFNoZWxsUm91dGUiLCJBdXRoTGF5b3V0IiwiTG9naW5QYWdlIiwiUmVnaXN0ZXJQYWdlIiwiUmVxdWlyZUF1dGgiLCJBcHAiLCJ2N19zdGFydFRyYW5zaXRpb24iLCJ2N19yZWxhdGl2ZVNwbGF0UGF0aCIsIl9jIiwiJFJlZnJlc2hSZWckIl0sImlnbm9yZUxpc3QiOltdLCJzb3VyY2VzIjpbIkFwcC50c3giXSwic291cmNlc0NvbnRlbnQiOlsiaW1wb3J0IHsgQnJvd3NlclJvdXRlciwgUm91dGUsIFJvdXRlcyB9IGZyb20gXCJyZWFjdC1yb3V0ZXItZG9tXCI7XG5pbXBvcnQgeyBBcHBTaGVsbFJvdXRlIH0gZnJvbSBcIi4vcm91dGVzL0FwcFNoZWxsUm91dGVcIjtcbmltcG9ydCB7IEF1dGhMYXlvdXQgfSBmcm9tIFwiLi9yb3V0ZXMvQXV0aExheW91dFwiO1xuaW1wb3J0IHsgTG9naW5QYWdlIH0gZnJvbSBcIi4vcm91dGVzL0xvZ2luUGFnZVwiO1xuaW1wb3J0IHsgUmVnaXN0ZXJQYWdlIH0gZnJvbSBcIi4vcm91dGVzL1JlZ2lzdGVyUGFnZVwiO1xuaW1wb3J0IHsgUmVxdWlyZUF1dGggfSBmcm9tIFwiLi9yb3V0ZXMvUmVxdWlyZUF1dGhcIjtcblxuZXhwb3J0IGZ1bmN0aW9uIEFwcCgpIHtcblx0cmV0dXJuIChcblx0XHQ8QnJvd3NlclJvdXRlclxuXHRcdFx0ZnV0dXJlPXt7XG5cdFx0XHRcdHY3X3N0YXJ0VHJhbnNpdGlvbjogdHJ1ZSxcblx0XHRcdFx0djdfcmVsYXRpdmVTcGxhdFBhdGg6IHRydWUsXG5cdFx0XHR9fVxuXHRcdD5cblx0XHRcdDxSb3V0ZXM+XG5cdFx0XHRcdDxSb3V0ZVxuXHRcdFx0XHRcdHBhdGg9XCIvbG9naW5cIlxuXHRcdFx0XHRcdGVsZW1lbnQ9e1xuXHRcdFx0XHRcdFx0PEF1dGhMYXlvdXQ+XG5cdFx0XHRcdFx0XHRcdDxMb2dpblBhZ2UgLz5cblx0XHRcdFx0XHRcdDwvQXV0aExheW91dD5cblx0XHRcdFx0XHR9XG5cdFx0XHRcdC8+XG5cdFx0XHRcdDxSb3V0ZVxuXHRcdFx0XHRcdHBhdGg9XCIvcmVnaXN0ZXJcIlxuXHRcdFx0XHRcdGVsZW1lbnQ9e1xuXHRcdFx0XHRcdFx0PEF1dGhMYXlvdXQ+XG5cdFx0XHRcdFx0XHRcdDxSZWdpc3RlclBhZ2UgLz5cblx0XHRcdFx0XHRcdDwvQXV0aExheW91dD5cblx0XHRcdFx0XHR9XG5cdFx0XHRcdC8+XG5cdFx0XHRcdDxSb3V0ZVxuXHRcdFx0XHRcdHBhdGg9XCIvKlwiXG5cdFx0XHRcdFx0ZWxlbWVudD17XG5cdFx0XHRcdFx0XHQ8UmVxdWlyZUF1dGg+XG5cdFx0XHRcdFx0XHRcdDxBcHBTaGVsbFJvdXRlIC8+XG5cdFx0XHRcdFx0XHQ8L1JlcXVpcmVBdXRoPlxuXHRcdFx0XHRcdH1cblx0XHRcdFx0Lz5cblx0XHRcdDwvUm91dGVzPlxuXHRcdDwvQnJvd3NlclJvdXRlcj5cblx0KTtcbn1cbiJdLCJmaWxlIjoiL2hvbWUvd2lzbXV0L2J5dGVvd2x6L29xdG9fcmVmYWN0b3IvZnJvbnRlbmQvc3JjL0FwcC50c3gifQ==