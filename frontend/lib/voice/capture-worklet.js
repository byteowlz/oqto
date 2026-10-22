// Shared Oqto capture processor. Kept as a same-origin asset for strict CSP;
// no recognition, buffering policy or destination selection belongs here.
class EarsCapture extends globalThis.AudioWorkletProcessor {
	process(inputs) {
		if (inputs[0]?.[0]) this.port.postMessage(inputs[0][0]);
		return true;
	}
}
globalThis.registerProcessor("ears-capture", EarsCapture);
