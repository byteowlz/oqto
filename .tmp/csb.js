import { createHotContext as __vite__createHotContext } from "/@vite/client";import.meta.hot = __vite__createHotContext("/features/chat/components/ChatSearchBar.tsx");"use client";
import __vite__cjsImport0_react_jsxDevRuntime from "/node_modules/.vite/deps/react_jsx-dev-runtime.js?v=92ce6bbd"; const jsxDEV = __vite__cjsImport0_react_jsxDevRuntime["jsxDEV"];
var _s = $RefreshSig$();
import { Button } from "/components/ui/button.tsx";
import { Input } from "/components/ui/input.tsx";
import { useSessionSearch } from "/hooks/use-session-search.ts?t=1775370528558";
import { cn } from "/lib/utils.ts";
import { ChevronDown, ChevronUp, Loader2, Search, X } from "/node_modules/.vite/deps/lucide-react.js?v=92ce6bbd";
import __vite__cjsImport6_react from "/node_modules/.vite/deps/react.js?v=92ce6bbd"; const useCallback = __vite__cjsImport6_react["useCallback"]; const useEffect = __vite__cjsImport6_react["useEffect"]; const useRef = __vite__cjsImport6_react["useRef"]; const useState = __vite__cjsImport6_react["useState"];
import { useTranslation } from "/node_modules/.vite/deps/react-i18next.js?v=92ce6bbd";
export function ChatSearchBar({
  sessionId,
  onResultSelect,
  className,
  isOpen,
  onToggle,
  locale = "en",
  hideCloseButton = false
}) {
  _s();
  const { t } = useTranslation();
  const inputRef = useRef(null);
  const [currentResultIndex, setCurrentResultIndex] = useState(0);
  const {
    query,
    setQuery,
    results,
    isSearching,
    error,
    clearSearch,
    isActive
  } = useSessionSearch({
    sessionId,
    debounceMs: 300,
    limit: 50
  });
  useEffect(() => {
    if (isOpen && inputRef.current) {
      inputRef.current.focus();
    }
  }, [isOpen]);
  const prevQueryRef = useRef(query);
  if (prevQueryRef.current !== query) {
    prevQueryRef.current = query;
    if (currentResultIndex !== 0) {
      setCurrentResultIndex(0);
    }
  }
  const currentResult = results[currentResultIndex];
  useEffect(() => {
    if (currentResult) {
      onResultSelect({
        lineNumber: currentResult.line_number,
        messageId: currentResult.message_id
      });
    }
  }, [currentResult, onResultSelect]);
  const handleClose = useCallback(() => {
    clearSearch();
    onToggle();
  }, [clearSearch, onToggle]);
  const handlePrev = useCallback(() => {
    if (results.length === 0) return;
    setCurrentResultIndex((prev) => prev > 0 ? prev - 1 : results.length - 1);
  }, [results.length]);
  const handleNext = useCallback(() => {
    if (results.length === 0) return;
    setCurrentResultIndex((prev) => prev < results.length - 1 ? prev + 1 : 0);
  }, [results.length]);
  const handleKeyDown = useCallback(
    (e) => {
      if (e.key === "Escape") {
        handleClose();
      } else if (e.key === "Enter") {
        if (e.shiftKey) {
          handlePrev();
        } else {
          handleNext();
        }
        e.preventDefault();
      }
    },
    [handleClose, handleNext, handlePrev]
  );
  if (!isOpen) {
    return /* @__PURE__ */ jsxDEV(
      Button,
      {
        variant: "ghost",
        size: "icon",
        onClick: onToggle,
        className: cn("h-8 w-8", className),
        title: "Search (Ctrl+F)",
        children: /* @__PURE__ */ jsxDEV(Search, { className: "h-4 w-4" }, void 0, false, {
          fileName: "/home/wismut/byteowlz/oqto_refactor/frontend/features/chat/components/ChatSearchBar.tsx",
          lineNumber: 128,
          columnNumber: 5
        }, this)
      },
      void 0,
      false,
      {
        fileName: "/home/wismut/byteowlz/oqto_refactor/frontend/features/chat/components/ChatSearchBar.tsx",
        lineNumber: 121,
        columnNumber: 7
      },
      this
    );
  }
  return /* @__PURE__ */ jsxDEV("div", { className: cn("flex items-center gap-2 p-2 bg-muted/30", className), children: [
    /* @__PURE__ */ jsxDEV(Search, { className: "h-4 w-4 text-muted-foreground flex-shrink-0" }, void 0, false, {
      fileName: "/home/wismut/byteowlz/oqto_refactor/frontend/features/chat/components/ChatSearchBar.tsx",
      lineNumber: 135,
      columnNumber: 4
    }, this),
    /* @__PURE__ */ jsxDEV(
      Input,
      {
        ref: inputRef,
        type: "text",
        value: query,
        onChange: (e) => setQuery(e.target.value),
        onKeyDown: handleKeyDown,
        placeholder: t("search.placeholder"),
        className: "h-7 text-sm border-0 bg-transparent shadow-none focus-visible:ring-0 focus-visible:ring-offset-0"
      },
      void 0,
      false,
      {
        fileName: "/home/wismut/byteowlz/oqto_refactor/frontend/features/chat/components/ChatSearchBar.tsx",
        lineNumber: 136,
        columnNumber: 4
      },
      this
    ),
    isActive && /* @__PURE__ */ jsxDEV("div", { className: "flex items-center gap-1 text-xs text-muted-foreground flex-shrink-0", children: isSearching ? /* @__PURE__ */ jsxDEV(Loader2, { className: "h-3 w-3 animate-spin" }, void 0, false, {
      fileName: "/home/wismut/byteowlz/oqto_refactor/frontend/features/chat/components/ChatSearchBar.tsx",
      lineNumber: 150,
      columnNumber: 9
    }, this) : results.length > 0 ? /* @__PURE__ */ jsxDEV("span", { children: [
      currentResultIndex + 1,
      "/",
      results.length
    ] }, void 0, true, {
      fileName: "/home/wismut/byteowlz/oqto_refactor/frontend/features/chat/components/ChatSearchBar.tsx",
      lineNumber: 152,
      columnNumber: 9
    }, this) : /* @__PURE__ */ jsxDEV("span", { children: t("search.noResults") }, void 0, false, {
      fileName: "/home/wismut/byteowlz/oqto_refactor/frontend/features/chat/components/ChatSearchBar.tsx",
      lineNumber: 156,
      columnNumber: 9
    }, this) }, void 0, false, {
      fileName: "/home/wismut/byteowlz/oqto_refactor/frontend/features/chat/components/ChatSearchBar.tsx",
      lineNumber: 148,
      columnNumber: 7
    }, this),
    results.length > 0 && /* @__PURE__ */ jsxDEV("div", { className: "flex items-center gap-0.5 flex-shrink-0", children: [
      /* @__PURE__ */ jsxDEV(
        Button,
        {
          variant: "ghost",
          size: "icon",
          onClick: handlePrev,
          className: "h-6 w-6",
          title: t("search.prev"),
          children: /* @__PURE__ */ jsxDEV(ChevronUp, { className: "h-3 w-3" }, void 0, false, {
            fileName: "/home/wismut/byteowlz/oqto_refactor/frontend/features/chat/components/ChatSearchBar.tsx",
            lineNumber: 171,
            columnNumber: 7
          }, this)
        },
        void 0,
        false,
        {
          fileName: "/home/wismut/byteowlz/oqto_refactor/frontend/features/chat/components/ChatSearchBar.tsx",
          lineNumber: 164,
          columnNumber: 6
        },
        this
      ),
      /* @__PURE__ */ jsxDEV(
        Button,
        {
          variant: "ghost",
          size: "icon",
          onClick: handleNext,
          className: "h-6 w-6",
          title: t("search.next"),
          children: /* @__PURE__ */ jsxDEV(ChevronDown, { className: "h-3 w-3" }, void 0, false, {
            fileName: "/home/wismut/byteowlz/oqto_refactor/frontend/features/chat/components/ChatSearchBar.tsx",
            lineNumber: 180,
            columnNumber: 7
          }, this)
        },
        void 0,
        false,
        {
          fileName: "/home/wismut/byteowlz/oqto_refactor/frontend/features/chat/components/ChatSearchBar.tsx",
          lineNumber: 173,
          columnNumber: 6
        },
        this
      )
    ] }, void 0, true, {
      fileName: "/home/wismut/byteowlz/oqto_refactor/frontend/features/chat/components/ChatSearchBar.tsx",
      lineNumber: 163,
      columnNumber: 7
    }, this),
    !hideCloseButton && /* @__PURE__ */ jsxDEV(
      Button,
      {
        variant: "ghost",
        size: "icon",
        onClick: handleClose,
        className: "h-6 w-6 flex-shrink-0",
        title: t("search.close"),
        children: /* @__PURE__ */ jsxDEV(X, { className: "h-3 w-3" }, void 0, false, {
          fileName: "/home/wismut/byteowlz/oqto_refactor/frontend/features/chat/components/ChatSearchBar.tsx",
          lineNumber: 194,
          columnNumber: 6
        }, this)
      },
      void 0,
      false,
      {
        fileName: "/home/wismut/byteowlz/oqto_refactor/frontend/features/chat/components/ChatSearchBar.tsx",
        lineNumber: 187,
        columnNumber: 7
      },
      this
    )
  ] }, void 0, true, {
    fileName: "/home/wismut/byteowlz/oqto_refactor/frontend/features/chat/components/ChatSearchBar.tsx",
    lineNumber: 134,
    columnNumber: 5
  }, this);
}
_s(ChatSearchBar, "bIkvKuFiQy9ecqCZvFKrFvwEsVs=", false, function() {
  return [useTranslation, useSessionSearch];
});
_c = ChatSearchBar;
var _c;
$RefreshReg$(_c, "ChatSearchBar");
import * as RefreshRuntime from "/@react-refresh";
const inWebWorker = typeof WorkerGlobalScope !== "undefined" && self instanceof WorkerGlobalScope;
if (import.meta.hot && !inWebWorker) {
  if (!window.$RefreshReg$) {
    throw new Error(
      "@vitejs/plugin-react can't detect preamble. Something is wrong."
    );
  }
  RefreshRuntime.__hmr_import(import.meta.url).then((currentExports) => {
    RefreshRuntime.registerExportsForReactRefresh("/home/wismut/byteowlz/oqto_refactor/frontend/features/chat/components/ChatSearchBar.tsx", currentExports);
    import.meta.hot.accept((nextExports) => {
      if (!nextExports) return;
      const invalidateMessage = RefreshRuntime.validateRefreshBoundaryAndEnqueueUpdate("/home/wismut/byteowlz/oqto_refactor/frontend/features/chat/components/ChatSearchBar.tsx", currentExports, nextExports);
      if (invalidateMessage) import.meta.hot.invalidate(invalidateMessage);
    });
  });
}
function $RefreshReg$(type, id) {
  return RefreshRuntime.register(type, "/home/wismut/byteowlz/oqto_refactor/frontend/features/chat/components/ChatSearchBar.tsx " + id);
}
function $RefreshSig$() {
  return RefreshRuntime.createSignatureFunctionForTransform();
}

//# sourceMappingURL=data:application/json;base64,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ZEQsZ0JBWVZYLGdCQUFnQjtBQUFBO0FBQUFxRCxLQXJCTHpDO0FBQWEsSUFBQXlDO0FBQUFDLGFBQUFELElBQUEiLCJuYW1lcyI6WyJfcyIsIiRSZWZyZXNoU2lnJCIsIkJ1dHRvbiIsIklucHV0IiwidXNlU2Vzc2lvblNlYXJjaCIsImNuIiwiQ2hldnJvbkRvd24iLCJDaGV2cm9uVXAiLCJMb2FkZXIyIiwiU2VhcmNoIiwiWCIsInVzZUNhbGxiYWNrIiwidXNlRWZmZWN0IiwidXNlUmVmIiwidXNlU3RhdGUiLCJ1c2VUcmFuc2xhdGlvbiIsIkNoYXRTZWFyY2hCYXIiLCJzZXNzaW9uSWQiLCJvblJlc3VsdFNlbGVjdCIsImNsYXNzTmFtZSIsImlzT3BlbiIsIm9uVG9nZ2xlIiwibG9jYWxlIiwiaGlkZUNsb3NlQnV0dG9uIiwidCIsImlucHV0UmVmIiwiY3VycmVudFJlc3VsdEluZGV4Iiwic2V0Q3VycmVudFJlc3VsdEluZGV4IiwicXVlcnkiLCJzZXRRdWVyeSIsInJlc3VsdHMiLCJpc1NlYXJjaGluZyIsImVycm9yIiwiY2xlYXJTZWFyY2giLCJpc0FjdGl2ZSIsImRlYm91bmNlTXMiLCJsaW1pdCIsImN1cnJlbnQiLCJmb2N1cyIsInByZXZRdWVyeVJlZiIsImN1cnJlbnRSZXN1bHQiLCJsaW5lTnVtYmVyIiwibGluZV9udW1iZXIiLCJtZXNzYWdlSWQiLCJtZXNzYWdlX2lkIiwiaGFuZGxlQ2xvc2UiLCJoYW5kbGVQcmV2IiwibGVuZ3RoIiwicHJldiIsImhhbmRsZU5leHQiLCJoYW5kbGVLZXlEb3duIiwiZSIsImtleSIsInNoaWZ0S2V5IiwicHJldmVudERlZmF1bHQiLCJ0YXJnZXQiLCJ2YWx1ZSIsIl9jIiwiJFJlZnJlc2hSZWckIl0sImlnbm9yZUxpc3QiOltdLCJzb3VyY2VzIjpbIkNoYXRTZWFyY2hCYXIudHN4Il0sInNvdXJjZXNDb250ZW50IjpbIlwidXNlIGNsaWVudFwiO1xuXG5pbXBvcnQgeyBCdXR0b24gfSBmcm9tIFwiQC9jb21wb25lbnRzL3VpL2J1dHRvblwiO1xuaW1wb3J0IHsgSW5wdXQgfSBmcm9tIFwiQC9jb21wb25lbnRzL3VpL2lucHV0XCI7XG5pbXBvcnQgeyB1c2VTZXNzaW9uU2VhcmNoIH0gZnJvbSBcIkAvaG9va3MvdXNlLXNlc3Npb24tc2VhcmNoXCI7XG5pbXBvcnQgeyBjbiB9IGZyb20gXCJAL2xpYi91dGlsc1wiO1xuaW1wb3J0IHsgQ2hldnJvbkRvd24sIENoZXZyb25VcCwgTG9hZGVyMiwgU2VhcmNoLCBYIH0gZnJvbSBcImx1Y2lkZS1yZWFjdFwiO1xuaW1wb3J0IHsgdXNlQ2FsbGJhY2ssIHVzZUVmZmVjdCwgdXNlUmVmLCB1c2VTdGF0ZSB9IGZyb20gXCJyZWFjdFwiO1xuaW1wb3J0IHsgdXNlVHJhbnNsYXRpb24gfSBmcm9tIFwicmVhY3QtaTE4bmV4dFwiO1xuXG5leHBvcnQgdHlwZSBDaGF0U2VhcmNoQmFyUHJvcHMgPSB7XG5cdC8qKiBTZXNzaW9uIElEIHRvIHNlYXJjaCB3aXRoaW4gKi9cblx0c2Vzc2lvbklkOiBzdHJpbmcgfCBudWxsO1xuXHQvKiogQ2FsbGJhY2sgd2hlbiBhIHJlc3VsdCBpcyBzZWxlY3RlZCAqL1xuXHRvblJlc3VsdFNlbGVjdDogKHJlc3VsdDogeyBsaW5lTnVtYmVyOiBudW1iZXI7IG1lc3NhZ2VJZD86IHN0cmluZyB9KSA9PiB2b2lkO1xuXHQvKiogQ2xhc3MgbmFtZSBmb3IgY29udGFpbmVyICovXG5cdGNsYXNzTmFtZT86IHN0cmluZztcblx0LyoqIFdoZXRoZXIgc2VhcmNoIGlzIGV4cGFuZGVkL3Zpc2libGUgKi9cblx0aXNPcGVuOiBib29sZWFuO1xuXHQvKiogQ2FsbGJhY2sgdG8gdG9nZ2xlIHNlYXJjaCB2aXNpYmlsaXR5ICovXG5cdG9uVG9nZ2xlOiAoKSA9PiB2b2lkO1xuXHQvKiogTG9jYWxlIGZvciB0cmFuc2xhdGlvbnMgKi9cblx0bG9jYWxlPzogXCJlblwiIHwgXCJkZVwiO1xuXHQvKiogSGlkZSB0aGUgY2xvc2UgYnV0dG9uICh3aGVuIHBhcmVudCBwcm92aWRlcyBpdHMgb3duKSAqL1xuXHRoaWRlQ2xvc2VCdXR0b24/OiBib29sZWFuO1xufTtcblxuLy8gVHJhbnNsYXRpb25zIG1vdmVkIHRvIG1lc3NhZ2VzL2VuLmpzb24gYW5kIG1lc3NhZ2VzL2RlLmpzb24gdW5kZXIgXCJzZWFyY2hcIiBzZWN0aW9uXG5cbi8qKlxuICogU2VhcmNoIGJhciBjb21wb25lbnQgZm9yIHNlYXJjaGluZyB3aXRoaW4gYSBjaGF0IHNlc3Npb24uXG4gKiBTaG93cyBpbmxpbmUgcmVzdWx0cyB3aXRoIG5hdmlnYXRpb24uXG4gKi9cbmV4cG9ydCBmdW5jdGlvbiBDaGF0U2VhcmNoQmFyKHtcblx0c2Vzc2lvbklkLFxuXHRvblJlc3VsdFNlbGVjdCxcblx0Y2xhc3NOYW1lLFxuXHRpc09wZW4sXG5cdG9uVG9nZ2xlLFxuXHRsb2NhbGUgPSBcImVuXCIsXG5cdGhpZGVDbG9zZUJ1dHRvbiA9IGZhbHNlLFxufTogQ2hhdFNlYXJjaEJhclByb3BzKSB7XG5cdGNvbnN0IHsgdCB9ID0gdXNlVHJhbnNsYXRpb24oKTtcblx0Y29uc3QgaW5wdXRSZWYgPSB1c2VSZWY8SFRNTElucHV0RWxlbWVudD4obnVsbCk7XG5cdGNvbnN0IFtjdXJyZW50UmVzdWx0SW5kZXgsIHNldEN1cnJlbnRSZXN1bHRJbmRleF0gPSB1c2VTdGF0ZSgwKTtcblxuXHRjb25zdCB7XG5cdFx0cXVlcnksXG5cdFx0c2V0UXVlcnksXG5cdFx0cmVzdWx0cyxcblx0XHRpc1NlYXJjaGluZyxcblx0XHRlcnJvcixcblx0XHRjbGVhclNlYXJjaCxcblx0XHRpc0FjdGl2ZSxcblx0fSA9IHVzZVNlc3Npb25TZWFyY2goe1xuXHRcdHNlc3Npb25JZCxcblx0XHRkZWJvdW5jZU1zOiAzMDAsXG5cdFx0bGltaXQ6IDUwLFxuXHR9KTtcblxuXHQvLyBGb2N1cyBpbnB1dCB3aGVuIG9wZW5lZFxuXHR1c2VFZmZlY3QoKCkgPT4ge1xuXHRcdGlmIChpc09wZW4gJiYgaW5wdXRSZWYuY3VycmVudCkge1xuXHRcdFx0aW5wdXRSZWYuY3VycmVudC5mb2N1cygpO1xuXHRcdH1cblx0fSwgW2lzT3Blbl0pO1xuXG5cdC8vIFJlc2V0IGluZGV4IHdoZW4gcXVlcnkgY2hhbmdlcyAtIHVzZSByZWYgdG8gYXZvaWQgZGVwZW5kZW5jeSBpc3N1ZXNcblx0Y29uc3QgcHJldlF1ZXJ5UmVmID0gdXNlUmVmKHF1ZXJ5KTtcblx0aWYgKHByZXZRdWVyeVJlZi5jdXJyZW50ICE9PSBxdWVyeSkge1xuXHRcdHByZXZRdWVyeVJlZi5jdXJyZW50ID0gcXVlcnk7XG5cdFx0aWYgKGN1cnJlbnRSZXN1bHRJbmRleCAhPT0gMCkge1xuXHRcdFx0c2V0Q3VycmVudFJlc3VsdEluZGV4KDApO1xuXHRcdH1cblx0fVxuXG5cdC8vIE5hdmlnYXRlIHRvIGN1cnJlbnQgcmVzdWx0XG5cdGNvbnN0IGN1cnJlbnRSZXN1bHQgPSByZXN1bHRzW2N1cnJlbnRSZXN1bHRJbmRleF07XG5cdHVzZUVmZmVjdCgoKSA9PiB7XG5cdFx0aWYgKGN1cnJlbnRSZXN1bHQpIHtcblx0XHRcdG9uUmVzdWx0U2VsZWN0KHtcblx0XHRcdFx0bGluZU51bWJlcjogY3VycmVudFJlc3VsdC5saW5lX251bWJlcixcblx0XHRcdFx0bWVzc2FnZUlkOiBjdXJyZW50UmVzdWx0Lm1lc3NhZ2VfaWQsXG5cdFx0XHR9KTtcblx0XHR9XG5cdH0sIFtjdXJyZW50UmVzdWx0LCBvblJlc3VsdFNlbGVjdF0pO1xuXG5cdGNvbnN0IGhhbmRsZUNsb3NlID0gdXNlQ2FsbGJhY2soKCkgPT4ge1xuXHRcdGNsZWFyU2VhcmNoKCk7XG5cdFx0b25Ub2dnbGUoKTtcblx0fSwgW2NsZWFyU2VhcmNoLCBvblRvZ2dsZV0pO1xuXG5cdGNvbnN0IGhhbmRsZVByZXYgPSB1c2VDYWxsYmFjaygoKSA9PiB7XG5cdFx0aWYgKHJlc3VsdHMubGVuZ3RoID09PSAwKSByZXR1cm47XG5cdFx0c2V0Q3VycmVudFJlc3VsdEluZGV4KChwcmV2KSA9PiAocHJldiA+IDAgPyBwcmV2IC0gMSA6IHJlc3VsdHMubGVuZ3RoIC0gMSkpO1xuXHR9LCBbcmVzdWx0cy5sZW5ndGhdKTtcblxuXHRjb25zdCBoYW5kbGVOZXh0ID0gdXNlQ2FsbGJhY2soKCkgPT4ge1xuXHRcdGlmIChyZXN1bHRzLmxlbmd0aCA9PT0gMCkgcmV0dXJuO1xuXHRcdHNldEN1cnJlbnRSZXN1bHRJbmRleCgocHJldikgPT4gKHByZXYgPCByZXN1bHRzLmxlbmd0aCAtIDEgPyBwcmV2ICsgMSA6IDApKTtcblx0fSwgW3Jlc3VsdHMubGVuZ3RoXSk7XG5cblx0Y29uc3QgaGFuZGxlS2V5RG93biA9IHVzZUNhbGxiYWNrKFxuXHRcdChlOiBSZWFjdC5LZXlib2FyZEV2ZW50KSA9PiB7XG5cdFx0XHRpZiAoZS5rZXkgPT09IFwiRXNjYXBlXCIpIHtcblx0XHRcdFx0aGFuZGxlQ2xvc2UoKTtcblx0XHRcdH0gZWxzZSBpZiAoZS5rZXkgPT09IFwiRW50ZXJcIikge1xuXHRcdFx0XHRpZiAoZS5zaGlmdEtleSkge1xuXHRcdFx0XHRcdGhhbmRsZVByZXYoKTtcblx0XHRcdFx0fSBlbHNlIHtcblx0XHRcdFx0XHRoYW5kbGVOZXh0KCk7XG5cdFx0XHRcdH1cblx0XHRcdFx0ZS5wcmV2ZW50RGVmYXVsdCgpO1xuXHRcdFx0fVxuXHRcdH0sXG5cdFx0W2hhbmRsZUNsb3NlLCBoYW5kbGVOZXh0LCBoYW5kbGVQcmV2XSxcblx0KTtcblxuXHRpZiAoIWlzT3Blbikge1xuXHRcdHJldHVybiAoXG5cdFx0XHQ8QnV0dG9uXG5cdFx0XHRcdHZhcmlhbnQ9XCJnaG9zdFwiXG5cdFx0XHRcdHNpemU9XCJpY29uXCJcblx0XHRcdFx0b25DbGljaz17b25Ub2dnbGV9XG5cdFx0XHRcdGNsYXNzTmFtZT17Y24oXCJoLTggdy04XCIsIGNsYXNzTmFtZSl9XG5cdFx0XHRcdHRpdGxlPVwiU2VhcmNoIChDdHJsK0YpXCJcblx0XHRcdD5cblx0XHRcdFx0PFNlYXJjaCBjbGFzc05hbWU9XCJoLTQgdy00XCIgLz5cblx0XHRcdDwvQnV0dG9uPlxuXHRcdCk7XG5cdH1cblxuXHRyZXR1cm4gKFxuXHRcdDxkaXYgY2xhc3NOYW1lPXtjbihcImZsZXggaXRlbXMtY2VudGVyIGdhcC0yIHAtMiBiZy1tdXRlZC8zMFwiLCBjbGFzc05hbWUpfT5cblx0XHRcdDxTZWFyY2ggY2xhc3NOYW1lPVwiaC00IHctNCB0ZXh0LW11dGVkLWZvcmVncm91bmQgZmxleC1zaHJpbmstMFwiIC8+XG5cdFx0XHQ8SW5wdXRcblx0XHRcdFx0cmVmPXtpbnB1dFJlZn1cblx0XHRcdFx0dHlwZT1cInRleHRcIlxuXHRcdFx0XHR2YWx1ZT17cXVlcnl9XG5cdFx0XHRcdG9uQ2hhbmdlPXsoZSkgPT4gc2V0UXVlcnkoZS50YXJnZXQudmFsdWUpfVxuXHRcdFx0XHRvbktleURvd249e2hhbmRsZUtleURvd259XG5cdFx0XHRcdHBsYWNlaG9sZGVyPXt0KFwic2VhcmNoLnBsYWNlaG9sZGVyXCIpfVxuXHRcdFx0XHRjbGFzc05hbWU9XCJoLTcgdGV4dC1zbSBib3JkZXItMCBiZy10cmFuc3BhcmVudCBzaGFkb3ctbm9uZSBmb2N1cy12aXNpYmxlOnJpbmctMCBmb2N1cy12aXNpYmxlOnJpbmctb2Zmc2V0LTBcIlxuXHRcdFx0Lz5cblxuXHRcdFx0ey8qIFJlc3VsdHMgaW5kaWNhdG9yICovfVxuXHRcdFx0e2lzQWN0aXZlICYmIChcblx0XHRcdFx0PGRpdiBjbGFzc05hbWU9XCJmbGV4IGl0ZW1zLWNlbnRlciBnYXAtMSB0ZXh0LXhzIHRleHQtbXV0ZWQtZm9yZWdyb3VuZCBmbGV4LXNocmluay0wXCI+XG5cdFx0XHRcdFx0e2lzU2VhcmNoaW5nID8gKFxuXHRcdFx0XHRcdFx0PExvYWRlcjIgY2xhc3NOYW1lPVwiaC0zIHctMyBhbmltYXRlLXNwaW5cIiAvPlxuXHRcdFx0XHRcdCkgOiByZXN1bHRzLmxlbmd0aCA+IDAgPyAoXG5cdFx0XHRcdFx0XHQ8c3Bhbj5cblx0XHRcdFx0XHRcdFx0e2N1cnJlbnRSZXN1bHRJbmRleCArIDF9L3tyZXN1bHRzLmxlbmd0aH1cblx0XHRcdFx0XHRcdDwvc3Bhbj5cblx0XHRcdFx0XHQpIDogKFxuXHRcdFx0XHRcdFx0PHNwYW4+e3QoXCJzZWFyY2gubm9SZXN1bHRzXCIpfTwvc3Bhbj5cblx0XHRcdFx0XHQpfVxuXHRcdFx0XHQ8L2Rpdj5cblx0XHRcdCl9XG5cblx0XHRcdHsvKiBOYXZpZ2F0aW9uIGJ1dHRvbnMgKi99XG5cdFx0XHR7cmVzdWx0cy5sZW5ndGggPiAwICYmIChcblx0XHRcdFx0PGRpdiBjbGFzc05hbWU9XCJmbGV4IGl0ZW1zLWNlbnRlciBnYXAtMC41IGZsZXgtc2hyaW5rLTBcIj5cblx0XHRcdFx0XHQ8QnV0dG9uXG5cdFx0XHRcdFx0XHR2YXJpYW50PVwiZ2hvc3RcIlxuXHRcdFx0XHRcdFx0c2l6ZT1cImljb25cIlxuXHRcdFx0XHRcdFx0b25DbGljaz17aGFuZGxlUHJldn1cblx0XHRcdFx0XHRcdGNsYXNzTmFtZT1cImgtNiB3LTZcIlxuXHRcdFx0XHRcdFx0dGl0bGU9e3QoXCJzZWFyY2gucHJldlwiKX1cblx0XHRcdFx0XHQ+XG5cdFx0XHRcdFx0XHQ8Q2hldnJvblVwIGNsYXNzTmFtZT1cImgtMyB3LTNcIiAvPlxuXHRcdFx0XHRcdDwvQnV0dG9uPlxuXHRcdFx0XHRcdDxCdXR0b25cblx0XHRcdFx0XHRcdHZhcmlhbnQ9XCJnaG9zdFwiXG5cdFx0XHRcdFx0XHRzaXplPVwiaWNvblwiXG5cdFx0XHRcdFx0XHRvbkNsaWNrPXtoYW5kbGVOZXh0fVxuXHRcdFx0XHRcdFx0Y2xhc3NOYW1lPVwiaC02IHctNlwiXG5cdFx0XHRcdFx0XHR0aXRsZT17dChcInNlYXJjaC5uZXh0XCIpfVxuXHRcdFx0XHRcdD5cblx0XHRcdFx0XHRcdDxDaGV2cm9uRG93biBjbGFzc05hbWU9XCJoLTMgdy0zXCIgLz5cblx0XHRcdFx0XHQ8L0J1dHRvbj5cblx0XHRcdFx0PC9kaXY+XG5cdFx0XHQpfVxuXG5cdFx0XHR7LyogQ2xvc2UgYnV0dG9uICovfVxuXHRcdFx0eyFoaWRlQ2xvc2VCdXR0b24gJiYgKFxuXHRcdFx0XHQ8QnV0dG9uXG5cdFx0XHRcdFx0dmFyaWFudD1cImdob3N0XCJcblx0XHRcdFx0XHRzaXplPVwiaWNvblwiXG5cdFx0XHRcdFx0b25DbGljaz17aGFuZGxlQ2xvc2V9XG5cdFx0XHRcdFx0Y2xhc3NOYW1lPVwiaC02IHctNiBmbGV4LXNocmluay0wXCJcblx0XHRcdFx0XHR0aXRsZT17dChcInNlYXJjaC5jbG9zZVwiKX1cblx0XHRcdFx0PlxuXHRcdFx0XHRcdDxYIGNsYXNzTmFtZT1cImgtMyB3LTNcIiAvPlxuXHRcdFx0XHQ8L0J1dHRvbj5cblx0XHRcdCl9XG5cdFx0PC9kaXY+XG5cdCk7XG59XG4iXSwiZmlsZSI6Ii9ob21lL3dpc211dC9ieXRlb3dsei9vcXRvX3JlZmFjdG9yL2Zyb250ZW5kL2ZlYXR1cmVzL2NoYXQvY29tcG9uZW50cy9DaGF0U2VhcmNoQmFyLnRzeCJ9