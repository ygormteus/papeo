// Testa a limpeza das mensagens de voz (`src-tauri/src/voice_enhance.js`) com o
// RNNoise de verdade e um Web Audio de mentira: `pnpm test`.
import { readFileSync } from "node:fs";
import { resolveObjectURL } from "node:buffer";

const wasm = readFileSync(new URL("../src-tauri/vendor/rnnoise/rnnoise.wasm", import.meta.url));
const source = readFileSync(new URL("../src-tauri/src/voice_enhance.js", import.meta.url), "utf8").replace(
  "__PAPEO_RNNOISE_WASM__",
  wasm.toString("base64"),
);

const failures = [];
const check = (ok, msg) => ok || failures.push(msg);

// AudioWorklet: o módulo roda com estes globais, como na thread de áudio.
let Processor = null;
// Mensagens do processador chegam ao nó da página depois (como entre threads).
let currentNode = null;
class FakePort {
  onmessage = null;
  constructor(peer) {
    this.peer = peer;
  }
  postMessage(data) {
    const node = this.peer;
    setTimeout(() => node?.port.onmessage?.({ data }), 0);
  }
}
class AudioWorkletProcessor {
  port = new FakePort(currentNode);
}
const runModule = async (url) => {
  const code = await resolveObjectURL(url).text();
  new Function("AudioWorkletProcessor", "registerProcessor", code)(AudioWorkletProcessor, (name, cls) => {
    check(name === "papeo-rnnoise", "nome do processador");
    Processor = cls;
  });
};

const param = () => ({ value: 0 });
const node = (extra = {}) => ({ connected: [], connect(n) { this.connected.push(n); }, ...extra });
let lastCtx = null;
let failContext = false;
class FakeContext {
  constructor(opts) {
    if (failContext) throw new Error("sem áudio");
    this.sampleRate = opts.sampleRate;
    this.state = "running";
    this.closed = false;
    this.audioWorklet = { addModule: runModule };
    lastCtx = this;
  }
  createMediaStreamSource(stream) {
    return node({ stream });
  }
  createBiquadFilter() {
    return node({ type: "", frequency: param(), Q: param() });
  }
  createDynamicsCompressor() {
    return node({ threshold: param(), knee: param(), ratio: param(), attack: param(), release: param() });
  }
  createGain() {
    return node({ gain: param() });
  }
  createMediaStreamDestination() {
    const track = { stopped: false, stop() { this.stopped = true; } };
    this.dest = node({ stream: { getTracks: () => [track] }, track });
    return this.dest;
  }
  async resume() {}
  async close() {
    this.closed = true;
  }
}
let processor = null;
let options = [];
class FakeWorkletNode {
  constructor(_ctx, name, opts) {
    check(name === "papeo-rnnoise", "nó usa o processador registrado");
    options.push(opts.processorOptions);
    this.port = { onmessage: null, postMessage: (data) => processor.port.onmessage?.({ data }) };
    currentNode = this;
    processor = new Processor(opts);
    currentNode = null;
    this.connected = [];
  }
  connect(n) {
    this.connected.push(n);
  }
}

globalThis.window = globalThis;
globalThis.AudioContext = FakeContext;
globalThis.AudioWorkletNode = FakeWorkletNode;
new Function(source)();

const mic = () => {
  const track = { stopped: false, stop() { this.stopped = true; }, addEventListener() {} };
  return { track, getTracks: () => [track] };
};

// Caminho completo: microfone → corte de graves → RNNoise → nivelador.
window.__papeoVoiceWarmup();
const input = mic();
const out = await window.__papeoEnhanceVoice(input);
check(out === lastCtx.dest.stream, "devolve o áudio tratado");
check(processor !== null && processor.wasm, "RNNoise ligado");
check(options[0].module instanceof WebAssembly.Module, "módulo compilado na página");

// O processador tira ruído e mantém o ritmo (128 amostras por bloco).
const rms = (a) => Math.sqrt(a.reduce((s, x) => s + x * x, 0) / a.length);
let inSum = 0;
let outSum = 0;
let blocks = 0;
// Ruído com semente fixa: o resultado não varia de uma execução para outra.
let seed = Number(process.env.SEED ?? 1);
const random = () => ((seed = (seed * 1664525 + 1013904223) >>> 0) / 2 ** 32);
for (let b = 0; b < 3000; b++) {
  const block = Float32Array.from({ length: 128 }, () => (random() * 2 - 1) * 0.05);
  const output = new Float32Array(128);
  check(processor.process([[block]], [[output]]) === true, "processador segue vivo");
  if (b > 1500) {
    inSum += rms(block);
    outSum += rms(output);
    blocks++;
  }
}
const reduction = outSum / inSum;
check(reduction < 0.6, `ruído reduzido (saída/entrada = ${reduction.toFixed(2)})`);
check(processor.size < 480 * 2, "fila do processador não cresce");

// Ruído constante que o RNNoise confunde com fala (semente 5): o nivelador
// não aumenta o volume dele.
{
  currentNode = { port: {} };
  const steady = new Processor({ processorOptions: { module: options[0].module } });
  currentNode = null;
  let s5 = 5;
  const r5 = () => ((s5 = (s5 * 1664525 + 1013904223) >>> 0) / 2 ** 32);
  let i5 = 0;
  let o5 = 0;
  for (let b = 0; b < 3000; b++) {
    const block = Float32Array.from({ length: 128 }, () => (r5() * 2 - 1) * 0.05);
    const output = new Float32Array(128);
    steady.process([[block]], [[output]]);
    if (b > 1500) {
      i5 += rms(block);
      o5 += rms(output);
    }
  }
  check(steady.gain < 1.2 && o5 <= i5, `ruído constante não é amplificado (ganho ${steady.gain.toFixed(2)})`);
}

// Parar a gravação solta o microfone e fecha o áudio.
out.getTracks()[0].stop();
check(input.track.stopped, "parar a gravação para o microfone");
check(lastCtx.closed, "parar a gravação fecha o processamento");
check(processor.process([[new Float32Array(128)]], [[new Float32Array(128)]]) === false, "processador encerra");

// Áudio que não liga: grava com o microfone original, sem travar.
class StuckContext extends FakeContext {
  constructor(opts) {
    super(opts);
    this.state = "suspended";
  }
  resume() {
    return new Promise(() => {});
  }
}
globalThis.AudioContext = StuckContext;
const stuck = mic();
check((await window.__papeoEnhanceVoice(stuck)) === stuck, "áudio parado devolve o microfone original");
check(lastCtx.closed, "áudio parado é fechado");
globalThis.AudioContext = FakeContext;

// RNNoise que não carrega no processador: devolve o microfone original e
// o processador deixa o som passar (nunca mudo).
const badModule = new WebAssembly.Module(
  Uint8Array.from([0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00]),
);
const RealWorklet = FakeWorkletNode;
globalThis.AudioWorkletNode = class extends RealWorklet {
  constructor(ctx, name, opts) {
    super(ctx, name, { ...opts, processorOptions: { module: badModule } });
  }
};
const broken = mic();
check((await window.__papeoEnhanceVoice(broken)) === broken, "RNNoise com falha devolve o microfone original");
check(lastCtx.closed, "RNNoise com falha fecha o áudio");
const through = Float32Array.from({ length: 128 }, (_, i) => Math.sin(i / 5) * 0.3);
const passed = new Float32Array(128);
processor.process([[through]], [[passed]]);
check(passed.every((x, i) => x === through[i]), "sem RNNoise o processador deixa o som passar");
globalThis.AudioWorkletNode = RealWorklet;

// Sem Web Audio: devolve o microfone original.
failContext = true;
const raw = mic();
check((await window.__papeoEnhanceVoice(raw)) === raw, "falha devolve o microfone original");

if (failures.length) {
  console.error(failures.join("\n"));
  process.exit(1);
}
console.log("limpeza de voz ok");
