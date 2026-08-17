import { cleanup, fireEvent, render } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { ThumbnailImage } from "../features/sessions/components/ThumbnailImage";

vi.mock("@/lib/mux-files", () => ({
	readFileMux: vi.fn(() => new Promise(() => {})),
}));

class IntersectionObserverStub {
	observe() {}
	unobserve() {}
	disconnect() {}
}
vi.stubGlobal("IntersectionObserver", IntersectionObserverStub);

afterEach(cleanup);

describe("ThumbnailImage click containment", () => {
	it("does not bubble gallery clicks to the surrounding card", () => {
		const onOpenInGallery = vi.fn();
		const onCardSelect = vi.fn();

		const { container } = render(
			<button type="button" onClick={onCardSelect} data-testid="card">
				<ThumbnailImage
					workspacePath="/tmp/ws"
					filePath="shots/one.png"
					filename="one.png"
					size={96}
					onClick={onOpenInGallery}
				/>
			</button>,
		);

		const thumbnail = container.querySelector('[style*="width: 96px"]');
		expect(thumbnail).not.toBeNull();
		fireEvent.click(thumbnail as HTMLElement);

		expect(onOpenInGallery).toHaveBeenCalledTimes(1);
		expect(onCardSelect).not.toHaveBeenCalled();
	});

	it("keeps plain thumbnails transparent to the card when no handler exists", () => {
		const onCardSelect = vi.fn();

		const { container } = render(
			<button type="button" onClick={onCardSelect}>
				<ThumbnailImage
					workspacePath="/tmp/ws"
					filePath="shots/two.png"
					filename="two.png"
					size={96}
				/>
			</button>,
		);

		const thumbnail = container.querySelector('[style*="width: 96px"]');
		fireEvent.click(thumbnail as HTMLElement);

		expect(onCardSelect).toHaveBeenCalledTimes(1);
	});
});
