import { useReducer } from "react";
import {
	type Adjustments,
	DEFAULT_ADJUSTMENTS,
	DEFAULT_LAYER,
	type ImageInfo,
	type ImageLayer,
	type Tool,
} from "../types";

export interface PhotoEditorState {
	image: ImageInfo | null;
	tool: Tool;
	adjustments: Adjustments;
	layer: ImageLayer;
}

export type PhotoEditorAction =
	| { type: "setImage"; image: ImageInfo }
	| { type: "setTool"; tool: Tool }
	| { type: "setAdjustment"; key: keyof Adjustments; value: number }
	| { type: "resetAdjustments" }
	| { type: "setLayerVisible"; visible: boolean }
	| { type: "setLayerOpacity"; opacity: number };

const initialState: PhotoEditorState = {
	image: null,
	tool: "move",
	adjustments: { ...DEFAULT_ADJUSTMENTS },
	layer: { ...DEFAULT_LAYER },
};

function reducer(
	state: PhotoEditorState,
	action: PhotoEditorAction,
): PhotoEditorState {
	switch (action.type) {
		case "setImage":
			return { ...state, image: action.image };
		case "setTool":
			return { ...state, tool: action.tool };
		case "setAdjustment":
			return {
				...state,
				adjustments: { ...state.adjustments, [action.key]: action.value },
			};
		case "resetAdjustments":
			return { ...state, adjustments: { ...DEFAULT_ADJUSTMENTS } };
		case "setLayerVisible":
			return { ...state, layer: { ...state.layer, visible: action.visible } };
		case "setLayerOpacity":
			return { ...state, layer: { ...state.layer, opacity: action.opacity } };
		default:
			return state;
	}
}

export function usePhotoEditorState() {
	return useReducer(reducer, initialState);
}
