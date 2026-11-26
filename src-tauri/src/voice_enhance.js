// Injetado só no WhatsApp (`webviews.rs`), junto com `notification_shim.js`.
// "Limpeza do Papeo" nas mensagens de voz: o microfone chega cru e passa por
// um corte de graves (ronco, batida na mesa), pela remoção de ruído do
// RNNoise (`src-tauri/vendor/rnnoise`) e por um nivelador de volume que só
// age quando o RNNoise detecta fala (não sobe o ruído nas pausas), antes de
// ir para o site. Começa na hora (não usa o processamento de voz do sistema)
// e funciona igual no WebKit (macOS) e no WebView2 (Windows). Qualquer falha
// devolve o microfone original: gravar nunca deixa de funcionar.
(() => {
  if (window.__papeoEnhanceVoice) return;

  // Trocado pelo binário em base64 ao injetar (`webviews.rs`).
  const WASM_BASE64 = "__PAPEO_RNNOISE_WASM__";

  // Roda na thread de áudio (AudioWorklet), como texto à parte.
  function processorModule() {
    const FRAME = 480; // o RNNoise trabalha em quadros de 10 ms a 48 kHz
    // Nivelador: acompanha o volume médio da fala (~0,5 s, só nos quadros em
    // que o RNNoise detecta fala) e o leva para perto de -20 dBFS, entre -6 e
    // +12 dB. Nas pausas o ganho fica parado: o ruído que sobrou não sobe.
    const TARGET = 0.1;
    const MIN_GAIN = 0.5;
    const MAX_GAIN = 4;
    const SPEECH = 0.6;
    const FLOOR = 0.01 * 0.01; // abaixo de -40 dBFS não é fala, é o ruído que sobrou
    const MODULATION = 1.5; // dB médios entre quadros: abaixo disso, som constante
    class PapeoRnnoise extends AudioWorkletProcessor {
      constructor(options) {
        super();
        this.alive = true;
        this.port.onmessage = (e) => {
          if (e.data === "stop") this.alive = false;
        };
        // Sem o RNNoise (falha ao carregar), o som passa direto: nunca mudo.
        this.wasm = null;
        try {
          let memory;
          const imports = {
            a: {
              a: () => 0, // emscripten_resize_heap: a memória inicial (32 MB) basta
              b: (dest, src, n) => new Uint8Array(memory.buffer).copyWithin(dest, src, src + n),
            },
          };
          const opts = options.processorOptions;
          const module = opts.module || new WebAssembly.Module(opts.wasm);
          const wasm = new WebAssembly.Instance(module, imports).exports;
          memory = wasm.c;
          wasm.d();
          this.state = wasm.f(0);
          this.ptr = wasm.g(FRAME * 4);
          this.wasm = wasm;
          this.port.postMessage("ready");
        } catch (_) {
          this.port.postMessage("failed");
        }
        this.input = new Float32Array(FRAME);
        this.filled = 0;
        this.queue = new Float32Array(FRAME * 8);
        this.head = 0;
        this.size = 0;
        this.gain = 1;
        this.level = TARGET * TARGET;
        this.flux = 0;
        this.lastDb = -120;
      }

      frame() {
        const heap = new Float32Array(this.wasm.c.buffer, this.ptr, FRAME);
        for (let i = 0; i < FRAME; i++) heap[i] = this.input[i] * 32768;
        const vad = this.wasm.j(this.state, this.ptr, this.ptr);
        const out = new Float32Array(this.wasm.c.buffer, this.ptr, FRAME);
        let sum = 0;
        for (let i = 0; i < FRAME; i++) sum += out[i] * out[i];
        const power = sum / FRAME / (32768 * 32768);
        // Fala sobe e desce vários dB de um quadro para o outro; ruído
        // constante quase não varia. Média da variação em ~0,3 s.
        const db = 10 * Math.log10(power + 1e-12);
        this.flux += (Math.abs(db - this.lastDb) - this.flux) * 0.03;
        this.lastDb = db;
        if (vad > SPEECH && power > FLOOR) {
          this.level += (power - this.level) * 0.02;
          const want = Math.min(MAX_GAIN, Math.max(MIN_GAIN, TARGET / Math.sqrt(this.level + 1e-12)));
          // Baixar o volume vale sempre; subir só com cara de fala (um ruído
          // constante que o RNNoise confunda com voz não é amplificado).
          if (want < this.gain || this.flux > MODULATION) this.gain += (want - this.gain) * 0.05;
        }
        const cap = this.queue.length;
        for (let i = 0; i < FRAME && this.size < cap; i++) {
          // Ganho e limite suave: picos acima de 0,9 são arredondados, sem estourar.
          let x = (out[i] / 32768) * this.gain;
          const a = Math.abs(x);
          if (a > 0.9) x = Math.sign(x) * (0.9 + 0.1 * Math.tanh((a - 0.9) / 0.1));
          this.queue[(this.head + this.size) % cap] = x;
          this.size++;
        }
      }

      process(inputs, outputs) {
        if (!this.alive) {
          if (this.wasm) {
            this.wasm.h(this.state);
            this.wasm.i(this.ptr);
            this.wasm = null;
          }
          return false;
        }
        const input = inputs[0] && inputs[0][0];
        const output = outputs[0] && outputs[0][0];
        if (!this.wasm) {
          if (input && output) output.set(input);
          return true;
        }
        if (input) {
          for (let i = 0; i < input.length; i++) {
            this.input[this.filled++] = input[i];
            if (this.filled === FRAME) {
              this.frame();
              this.filled = 0;
            }
          }
        }
        if (output) {
          const cap = this.queue.length;
          for (let i = 0; i < output.length; i++) {
            if (this.size > 0) {
              output[i] = this.queue[this.head];
              this.head = (this.head + 1) % cap;
              this.size--;
            } else {
              output[i] = 0;
            }
          }
        }
        return true;
      }
    }
    registerProcessor("papeo-rnnoise", PapeoRnnoise);
  }

  let wasmBytes = null;
  const bytes = () => {
    if (!wasmBytes) {
      const bin = atob(WASM_BASE64);
      const arr = new Uint8Array(bin.length);
      for (let i = 0; i < bin.length; i++) arr[i] = bin.charCodeAt(i);
      wasmBytes = arr.buffer;
    }
    return wasmBytes.slice(0);
  };
  // Compilado uma vez na página (fora da thread de áudio). Se a página não
  // deixar compilar WebAssembly, fica sem a remoção de ruído.
  let compiled = null;
  const wasmModule = () => (compiled ??= WebAssembly.compile(bytes()).catch(() => null));
  let moduleUrl = null;
  const workletUrl = () =>
    (moduleUrl ??= URL.createObjectURL(new Blob([`(${processorModule})()`], { type: "text/javascript" })));

  // Preparado no toque do botão de gravar, enquanto o microfone ainda abre.
  let pending = null;
  const prepare = () => {
    if (pending) return pending;
    const ready = (async () => {
      const ctx = new AudioContext({ sampleRate: 48000, latencyHint: "interactive" });
      let denoise = false;
      let module = null;
      if (ctx.sampleRate === 48000 && ctx.audioWorklet) {
        try {
          [module] = await Promise.all([wasmModule(), ctx.audioWorklet.addModule(workletUrl())]);
          denoise = module !== null;
        } catch (_) {
          denoise = false;
        }
      }
      return { ctx, denoise, module };
    })();
    pending = ready;
    // Ninguém usou (o clique não virou gravação): fecha.
    setTimeout(() => {
      if (pending !== ready) return;
      pending = null;
      ready.then(({ ctx }) => ctx.close()).catch(() => {});
    }, 10000);
    return ready;
  };

  window.__papeoVoiceWarmup = () => {
    try {
      prepare().catch(() => {});
    } catch (_) {
      /* sem Web Audio: a gravação segue sem limpeza */
    }
  };

  window.__papeoEnhanceVoice = async (stream) => {
    let ctx = null;
    try {
      const ready = prepare();
      pending = null;
      const prepared = await ready;
      ctx = prepared.ctx;
      // Sem o RNNoise, a limpeza não vale a pena: o site grava com o
      // microfone normal (`notification_shim.js` pede de novo).
      if (!prepared.denoise) throw new Error("sem RNNoise");
      // Áudio que não liga (sem o toque do usuário, o WebKit pode segurar o
      // `resume`): grava com o microfone normal em vez de travar.
      if (ctx.state !== "running") {
        await Promise.race([ctx.resume(), new Promise((r) => setTimeout(r, 500))]);
        if (ctx.state !== "running") throw new Error("áudio parado");
      }
      const options = (processorOptions) => ({
        numberOfInputs: 1,
        numberOfOutputs: 1,
        outputChannelCount: [1],
        channelCount: 1,
        channelCountMode: "explicit",
        processorOptions,
      });
      let worklet;
      try {
        worklet = new AudioWorkletNode(ctx, "papeo-rnnoise", options({ module: prepared.module }));
      } catch (_) {
        // Navegador que não passa o módulo compilado para a thread de áudio:
        // vão os bytes, compilados lá.
        worklet = new AudioWorkletNode(ctx, "papeo-rnnoise", options({ wasm: bytes() }));
      }
      // O processador avisa se o RNNoise carregou; sem aviso, não arrisca.
      const loaded = await new Promise((resolve) => {
        const timer = setTimeout(() => resolve(false), 500);
        worklet.port.onmessage = (e) => {
          clearTimeout(timer);
          resolve(e.data === "ready");
        };
        worklet.onprocessorerror = () => {
          clearTimeout(timer);
          resolve(false);
        };
      });
      if (!loaded) throw new Error("RNNoise não carregou");

      // Corta abaixo de 80 Hz (ronco, vento, batida na mesa) → RNNoise → site.
      const source = ctx.createMediaStreamSource(stream);
      const highpass = ctx.createBiquadFilter();
      highpass.type = "highpass";
      highpass.frequency.value = 80;
      highpass.Q.value = 0.7;
      const dest = ctx.createMediaStreamDestination();
      source.connect(highpass);
      highpass.connect(worklet);
      worklet.connect(dest);

      // Parar a gravação (o site para as faixas devolvidas) solta o microfone
      // de verdade e fecha o processamento.
      let closed = false;
      const close = () => {
        if (closed) return;
        closed = true;
        for (const t of stream.getTracks()) t.stop();
        worklet.port.postMessage("stop");
        ctx.close().catch(() => {});
      };
      for (const track of dest.stream.getTracks()) {
        const stop = track.stop.bind(track);
        track.stop = () => {
          stop();
          close();
        };
      }
      for (const track of stream.getTracks()) {
        track.addEventListener("ended", () => {
          for (const t of dest.stream.getTracks()) t.stop();
        });
      }
      return dest.stream;
    } catch (_) {
      ctx?.close().catch(() => {});
      return stream;
    }
  };
})();
