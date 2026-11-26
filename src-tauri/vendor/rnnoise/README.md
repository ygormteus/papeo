# RNNoise (WASM)

Remoção de ruído usada na gravação de mensagens de voz ("limpeza do Papeo",
`src-tauri/src/voice_enhance.js`).

- Origem: pacote npm `@jitsi/rnnoise-wasm@0.2.1` (`dist/rnnoise.wasm`),
  <https://github.com/jitsi/rnnoise-wasm>, que compila o
  [RNNoise](https://gitlab.xiph.org/xiph/rnnoise) (Xiph.Org, BSD-3-Clause).
- Licenças: o pacote do Jitsi é Apache-2.0 (`LICENSE`); o RNNoise é
  BSD-3-Clause (`COPYING-rnnoise`, copiado do repositório atual do RNNoise;
  os avisos de copyright acompanham o programa distribuído).
- SHA-256 de `rnnoise.wasm`: `677147b9248aedc0a00de79dc090d1ecead37c08c8bb773a790da16109d2c487`
- Exportações (nomes minificados pelo Emscripten): `c` memória, `d`
  construtores, `f` rnnoise_create, `g` malloc, `h` rnnoise_destroy,
  `i` free, `j` rnnoise_process_frame. Importações: `a.a`
  emscripten_resize_heap, `a.b` emscripten_memcpy_big.
