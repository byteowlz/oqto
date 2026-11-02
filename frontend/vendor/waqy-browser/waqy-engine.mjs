// Streaming wake-phrase engine for the browser (CPU / WASM) and for bun tests.
//
// Pipeline per 80 ms frame of 16 kHz audio (1280 samples):
//   raw log-mel (8 new frames from the last 1760 samples), floored at (loudest frame in the
//   trailing 2 s - 80 dB), then x/10 + 2 -- identical to training/build_data.py StreamingMel
//   speech embedding (1 new 96-dim vector from the last 76 mel frames)
//   classifier over the last 16 embeddings -> score in [0, 1]
// This is the openWakeWord streaming layout used by livekit-wakeword; the mel graph is our
// formula-built melspectrogram_raw.onnx and the embedding is Google's Apache-2.0 speech_embedding.
//
// The engine only reports scores and threshold crossings. A detection is evidence that the
// phrase was probably spoken; it is never authentication or permission to act.

export const RATE = 16000;
export const FRAME = 1280;
const MEL_CONTEXT = 480; // 3 hops of overlap so each frame yields exactly 8 mel frames
const MEL_WINDOW = 76;
const EMB_WINDOW = 16;
const MEL_BINS = 32;
const EMB_DIM = 96;
const FLOOR_FRAMES = 200;
const TOP_DB = 80;
// Inaudible white noise (about -70 dBFS) on every input sample, as in the Rust detector:
// exact digital silence before speech made the heads fire on look-alikes.
export const DEFAULT_DITHER = 3e-4;

export class WakeEngine {
  // ort: an onnxruntime-web (or compatible) module. models: { mel, embedding, classifiers: {name: bytes|url} }
  static async create(ort, models, { threshold = 0.5, refractoryMs = 2000, dither = DEFAULT_DITHER, sessionOptions } = {}) {
    const options = { executionProviders: ['wasm'], graphOptimizationLevel: 'all', ...sessionOptions };
    const open = (m) => ort.InferenceSession.create(m, options);
    const mel = await open(models.mel);
    const embedding = await open(models.embedding);
    const classifiers = {};
    for (const [name, model] of Object.entries(models.classifiers)) classifiers[name] = await open(model);
    const engine = new WakeEngine(ort, mel, embedding, classifiers, threshold, refractoryMs);
    engine.dither = dither;
    return engine;
  }

  constructor(ort, mel, embedding, classifiers, threshold, refractoryMs) {
    this.ort = ort;
    this.mel = mel;
    this.embedding = embedding;
    this.classifiers = classifiers;
    this.threshold = threshold;
    this.refractoryFrames = Math.round(refractoryMs / 80);
    this.dither = DEFAULT_DITHER;
    this.seed = 0x9e3779b9;
    this.reset();
  }

  reset() {
    this.frameIndex = 0;
    this.pausedUntil = -1; // half-duplex: frames before this frame index are dropped (own playback + echo tail)
    this.clearState();
  }

  /** Forget audio context and lookback, keep the frame clock (timestamps stay monotonic). */
  clearState() {
    this.pending = new Float32Array(0);
    this.audio = new Float32Array(FRAME + MEL_CONTEXT); // rolling raw context, starts silent
    this.melFrames = []; // Float32Array(32) each
    this.peaks = []; // per-mel-frame max dB over the trailing 2 s
    this.embeddings = []; // Float32Array(96) each
    this.cooldown = Object.fromEntries(Object.keys(this.classifiers).map((n) => [n, 0]));
  }

  /** Half-duplex gate: while the app plays audio through the speakers, the microphone hears it too.
   *  Drop incoming audio from now until `tailMs` after playback ends, and forget the lookback so the
   *  tail cannot leak into a later decision. Returns the frame index when listening resumes. */
  setPlayback(active, { tailMs = 350 } = {}) {
    if (active) this.pausedUntil = Infinity;
    else if (this.pausedUntil === Infinity) { this.clearState(); this.pausedUntil = this.frameIndex + Math.round(tailMs / 80); }
    return this.pausedUntil;
  }

  get paused() { return this.frameIndex < this.pausedUntil; }

  // Push 16 kHz mono float samples in [-1, 1]. Returns one result per completed 80 ms frame:
  // { frame, timeMs, scores: {name: score}, detections: [name] }.
  #gaussian() {
    const next = () => { // mulberry32
      let t = (this.seed = (this.seed + 0x6d2b79f5) >>> 0);
      t = Math.imul(t ^ (t >>> 15), t | 1);
      t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
      return (((t ^ (t >>> 14)) >>> 0) + 0.5) / 4294967296;
    };
    return Math.sqrt(-2 * Math.log(next())) * Math.cos(2 * Math.PI * next());
  }

  async push(samples) {
    const joined = new Float32Array(this.pending.length + samples.length);
    joined.set(this.pending);
    joined.set(samples, this.pending.length);
    if (this.dither > 0) for (let i = this.pending.length; i < joined.length; i++) joined[i] += this.#gaussian() * this.dither;
    const results = [];
    let offset = 0;
    while (joined.length - offset >= FRAME) {
      results.push(await this.#step(joined.subarray(offset, offset + FRAME)));
      offset += FRAME;
    }
    this.pending = joined.slice(offset);
    return results.filter(Boolean);
  }

  async #step(frame) {
    // own playback: the clock advances, nothing is scored or remembered
    if (this.paused) { this.frameIndex++; return null; }
    this.audio.copyWithin(0, FRAME);
    this.audio.set(frame, MEL_CONTEXT);
    const index = this.frameIndex++;

    const melOut = await this.mel.run({ input: new this.ort.Tensor('float32', this.audio.slice(), [1, this.audio.length]) });
    const melData = Object.values(melOut)[0].data; // [1,1,8,32]
    const nNew = melData.length / MEL_BINS;
    for (let i = 0; i < nNew; i++) {
      let peak = -Infinity;
      for (let j = 0; j < MEL_BINS; j++) peak = Math.max(peak, melData[i * MEL_BINS + j]);
      this.peaks.push(peak);
      if (this.peaks.length > FLOOR_FRAMES) this.peaks.shift();
      const floor = Math.max(...this.peaks) - TOP_DB;
      const row = new Float32Array(MEL_BINS);
      for (let j = 0; j < MEL_BINS; j++) row[j] = Math.max(melData[i * MEL_BINS + j], floor) / 10 + 2;
      this.melFrames.push(row);
    }
    if (this.melFrames.length > MEL_WINDOW) this.melFrames.splice(0, this.melFrames.length - MEL_WINDOW);
    if (this.melFrames.length < MEL_WINDOW) return null;

    const window = new Float32Array(MEL_WINDOW * MEL_BINS);
    this.melFrames.forEach((row, i) => window.set(row, i * MEL_BINS));
    const embOut = await this.embedding.run({ input_1: new this.ort.Tensor('float32', window, [1, MEL_WINDOW, MEL_BINS, 1]) });
    this.embeddings.push(Float32Array.from(Object.values(embOut)[0].data));
    if (this.embeddings.length > EMB_WINDOW) this.embeddings.shift();
    if (this.embeddings.length < EMB_WINDOW) return null;

    const feats = new Float32Array(EMB_WINDOW * EMB_DIM);
    this.embeddings.forEach((e, i) => feats.set(e, i * EMB_DIM));
    const input = new this.ort.Tensor('float32', feats, [1, EMB_WINDOW, EMB_DIM]);
    const scores = {};
    const detections = [];
    for (const [name, session] of Object.entries(this.classifiers)) {
      const out = await session.run({ embeddings: input });
      const score = Object.values(out)[0].data[0];
      scores[name] = score;
      if (this.cooldown[name] > 0) this.cooldown[name]--;
      else if (score >= this.threshold) {
        detections.push(name);
        this.cooldown[name] = this.refractoryFrames;
      }
    }
    return { frame: index, timeMs: (index + 1) * 80, scores, detections };
  }
}
