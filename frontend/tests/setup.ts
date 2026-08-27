import "@testing-library/jest-dom";
import { cleanup } from "@testing-library/react";
import { afterEach, vi } from "vitest";

// Cleanup after each test case
afterEach(() => {
	cleanup();
});

// Mock fetch globally
global.fetch = vi.fn();

// Mock localStorage
const localStorageMock = {
	getItem: vi.fn(),
	setItem: vi.fn(),
	removeItem: vi.fn(),
	clear: vi.fn(),
	length: 0,
	key: vi.fn(),
};
global.localStorage = localStorageMock as unknown as Storage;

// @tanstack/react-virtual observes element rects; jsdom has neither API.
class ResizeObserverMock {
	observe() {}
	unobserve() {}
	disconnect() {}
}
if (typeof global.ResizeObserver === "undefined") {
	global.ResizeObserver =
		ResizeObserverMock as unknown as typeof global.ResizeObserver;
}
if (!("scrollTo" in Element.prototype)) {
	Object.defineProperty(Element.prototype, "scrollTo", { value: () => {} });
}

// jsdom has no matchMedia; hooks like useIsMobile need it.
if (typeof window.matchMedia === "undefined") {
	window.matchMedia = ((query: string) =>
		({
			matches: false,
			media: query,
			onchange: null,
			addListener: () => {},
			removeListener: () => {},
			addEventListener: () => {},
			removeEventListener: () => {},
			dispatchEvent: () => false,
		}) as unknown as MediaQueryList) as typeof window.matchMedia;
}
