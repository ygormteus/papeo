# 13 — Mensagens de voz

- **Estado:** entregue (0.11), retroativa

## Problema

No WhatsApp dentro da webview, o começo do áudio saía mudo (~1 s para abrir o
microfone) e o som vinha com ruído.

## Requisitos

- **R1.** Quando a pessoa tocar no botão de gravar do WhatsApp, o Papeo deve
  abrir o microfone na hora (aquecimento) e soltá-lo em 3 s se a gravação
  não começar.
- **R2.** Onde a "limpeza do Papeo" estiver escolhida, o Papeo deve passar a
  voz por filtro passa-altas de 80 Hz, RNNoise (48 kHz, quadros de 480) e
  nivelador com limite suave.
- **R3.** Se o RNNoise não carregar em 0,5 s, então o Papeo deve entregar o
  microfone sem tratamento (nunca mudo).
- **R4.** O modo escolhido deve continuar valendo depois de recarregar a
  página.
- **R5.** O nivelador só deve subir o ganho com variação de fala (≥ 1,5 dB
  em média entre quadros), para não amplificar ruído constante.

## Design

`voice_enhance.js` (worklet `papeo-rnnoise`, wasm embutido), shim
(`getUserMedia`, `pointerdown` no rodapé), `webviews.rs` (injeção e
reaplicação em `on_page_load`), `settings.voice_capture`.

## Verificação

| Req. | Evidência | macOS | Windows |
|---|---|---|---|
| R2–R3, R5 | `voice-enhance.test.mjs` (Web Audio simulado, ruído com semente fixa) | — | — |
| R1 | `notification-shim.test.mjs` (microfone na mensagem de voz) | ok (uso real) | não verificado |
| Qualidade audível | — | não verificado | não verificado |
