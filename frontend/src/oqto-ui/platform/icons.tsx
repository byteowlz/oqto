import { type SVGProps, forwardRef } from "react";
import { sharpPaths } from "./icons/sharp-paths";

export type OqtoIconName = keyof typeof sharpPaths;
export type OqtoIconProps = Omit<SVGProps<SVGSVGElement>, "name"> & {
	name: OqtoIconName;
	size?: number | string;
	rotation?: 0 | 90 | 180 | 270;
};

/** Exact selected iOS vectors; parent controls retain labels and hit targets. */
export const OqtoIcon = forwardRef<SVGSVGElement, OqtoIconProps>(
	function OqtoIcon({ name, size = 24, rotation = 0, ...props }, ref) {
		const glyph = sharpPaths[name];
		const [x, y, width, height] = glyph.viewBox.split(" ").map(Number);
		return (
			<svg
				ref={ref}
				width={size}
				height={size}
				viewBox={glyph.viewBox}
				fill="currentColor"
				stroke="none"
				aria-hidden="true"
				{...props}
			>
				<g
					transform={
						rotation
							? `rotate(${rotation} ${x + width / 2} ${y + height / 2})`
							: undefined
					}
				>
					{glyph.paths.map((d) => (
						<path key={d} d={d} />
					))}
				</g>
			</svg>
		);
	},
);
