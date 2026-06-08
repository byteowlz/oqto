import { useReducer } from "react";
import {
	type GridState,
	type Margins,
	type PoolImage,
	ZERO_MARGINS,
	emptyTile,
	evenSizes,
	makeTiles,
} from "../types";

export type PhotoGridAction =
	| { type: "setSpec"; rows: number; cols: number }
	| { type: "addPoolImages"; images: PoolImage[] }
	| { type: "removePoolImage"; id: string }
	| { type: "assignTile"; index: number; imageId: string }
	| { type: "clearTile"; index: number }
	| { type: "setTilePos"; index: number; posX: number; posY: number }
	| { type: "setColSizes"; sizes: number[] }
	| { type: "setRowSizes"; sizes: number[] }
	| { type: "setMargins"; margins: Margins }
	| { type: "setGap"; gap: number };

function createInitialState(): GridState {
	return {
		spec: { rows: 2, cols: 2 },
		colSizes: evenSizes(2),
		rowSizes: evenSizes(2),
		tiles: makeTiles(4),
		pool: [],
		gap: 4,
		margins: { ...ZERO_MARGINS },
	};
}

function reducer(state: GridState, action: PhotoGridAction): GridState {
	switch (action.type) {
		case "setSpec": {
			const count = action.rows * action.cols;
			const tiles = Array.from(
				{ length: count },
				(_, i) => state.tiles[i] ?? emptyTile(),
			);
			return {
				...state,
				spec: { rows: action.rows, cols: action.cols },
				colSizes: evenSizes(action.cols),
				rowSizes: evenSizes(action.rows),
				tiles,
			};
		}
		case "addPoolImages":
			return { ...state, pool: [...state.pool, ...action.images] };
		case "removePoolImage":
			return {
				...state,
				pool: state.pool.filter((p) => p.id !== action.id),
				tiles: state.tiles.map((t) =>
					t.imageId === action.id ? { ...t, imageId: null } : t,
				),
			};
		case "assignTile":
			return {
				...state,
				tiles: state.tiles.map((t, i) =>
					i === action.index
						? { imageId: action.imageId, posX: 50, posY: 50 }
						: t,
				),
			};
		case "clearTile":
			return {
				...state,
				tiles: state.tiles.map((t, i) =>
					i === action.index ? { ...t, imageId: null } : t,
				),
			};
		case "setTilePos":
			return {
				...state,
				tiles: state.tiles.map((t, i) =>
					i === action.index
						? { ...t, posX: action.posX, posY: action.posY }
						: t,
				),
			};
		case "setColSizes":
			return { ...state, colSizes: action.sizes };
		case "setRowSizes":
			return { ...state, rowSizes: action.sizes };
		case "setMargins":
			return { ...state, margins: action.margins };
		case "setGap":
			return { ...state, gap: action.gap };
		default:
			return state;
	}
}

export function usePhotoGridState() {
	return useReducer(reducer, undefined, createInitialState);
}
