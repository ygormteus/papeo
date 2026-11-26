// Testa o script injetado nos serviços (notificações e sinal de vida): `pnpm test`.
import { readFileSync } from "node:fs";

const source = readFileSync(new URL("../src-tauri/src/notification_shim.js", import.meta.url), "utf8");
const calls = [];
const timers = [];
const listeners = {};
let qrVisible = false;

globalThis.window = globalThis;
const opened = [];
window.open = (url, target, features) => (opened.push([url, target, features]), null);
window.__TAURI_INTERNALS__ = { invoke: (cmd, args) => (calls.push([cmd, args]), Promise.resolve()) };
window.setInterval = (fn) => timers.push(fn);
window.addEventListener = (type, fn) => (listeners[type] = fn);
globalThis.location = { hostname: "web.whatsapp.com" };
const docListeners = {};
const composer = { isContentEditable: true, innerText: "", focus() {} };
const sendButton = {
  closest: () => sendButton,
  dispatchEvent: (e) => e.type === "click" && (composer.innerText = ""),
};
const dom = {
  '#main footer [contenteditable="true"][role="textbox"]': composer,
  '#main footer button[aria-label="Send"]': sendButton,
};
const pressed = [];
const chatTitles = [{ getAttribute: () => "Ana Souza", dispatchEvent: (e) => pressed.push(e.type) }];
globalThis.MouseEvent = class {
  constructor(type) {
    this.type = type;
  }
};
globalThis.document = {
  visibilityState: "visible",
  querySelector: (sel) => (qrVisible && sel.includes("canvas") ? {} : (dom[sel] ?? null)),
  execCommand: (cmd, _ui, text) => {
    if (cmd === "insertText") composer.innerText += text;
    if (cmd === "insertLineBreak") composer.innerText += "\n";
    if (cmd === "delete") composer.innerText = "";
  },
  querySelectorAll: () => chatTitles,
  addEventListener: (type, fn) => (docListeners[type] = fn),
};
class SWR {}
window.ServiceWorkerRegistration = SWR;
let micLive = true;
let asked;
let requests = [];
Object.defineProperty(globalThis, "navigator", {
  value: {
    permissions: { query: async (d) => ({ state: "prompt", d }) },
    mediaDevices: {
      getUserMedia: async (c) => (
        (asked = c),
        requests.push(c),
        (() => {
          const wanted = typeof c.audio === "object" ? c.audio.deviceId : undefined;
          const device = (typeof wanted === "object" ? wanted?.exact : wanted) || "default";
          const track = {
            get readyState() { return micLive && !this.stopped ? "live" : "ended"; },
            stopped: false,
            stop() { this.stopped = true; },
            getSettings: () => ({ deviceId: device }),
          };
          return { track, getTracks: () => [track], getAudioTracks: () => [track] };
        })()
      ),
    },
  },
  configurable: true,
});
class FakeMedia {
  play() {
    this.paused = false;
    this.ended = false;
    return Promise.resolve();
  }
}
globalThis.HTMLMediaElement = FakeMedia;

eval(source);
eval(source); // idempotente: não instala duas vezes

const failures = [];
const check = (ok, msg) => ok || failures.push(msg);

// Notificações
let shown = 0;
const n = new Notification("Ana", { body: "Oi!", tag: "x" });
n.onshow = () => shown++;
let closed = 0;
const n2 = new Notification("Sem opções", null);
n2.addEventListener("close", () => closed++);
n2.close();
await Promise.resolve();
await new SWR().showNotification("Grupo", { body: 42, silent: true });
const notify = calls.filter(([c]) => c === "service_notify").map(([, a]) => a);
check(JSON.stringify(notify[0]) === JSON.stringify({ title: "Ana", body: "Oi!", silent: false }), "notificação simples");
check(JSON.stringify(notify[1]) === JSON.stringify({ title: "Sem opções", body: "", silent: false }), "options nulo");
check(JSON.stringify(notify[2]) === JSON.stringify({ title: "Grupo", body: "42", silent: true }), "service worker");
check(shown === 1 && closed === 1, "eventos show/close");
check(Notification.permission === "granted", "permission");
check((await Notification.requestPermission()) === "granted", "requestPermission");
check((await navigator.permissions.query({ name: "notifications" })).state === "granted", "permissions.query");
check((await navigator.permissions.query({ name: "camera" })).state === "prompt", "outras permissões intactas");

// Sinal de vida e sessão desconectada
check(timers.length === 2, "timers: sinal de vida e leitura da lista de conversas");
timers[0]();
qrVisible = true;
timers[0]();
const status = calls.filter(([c]) => c === "service_status").map(([, a]) => a.loggedOut);
check(JSON.stringify(status) === "[false,true]", `sinal de vida/deslogado: ${JSON.stringify(status)}`);
const media = calls.filter(([c]) => c === "service_status").map(([, a]) => a.media);
check(JSON.stringify(media) === "[false,false]", `sem mídia tocando: ${JSON.stringify(media)}`);
check(typeof docListeners.play === "function", "sinal de vida quando uma mídia começa");
check(typeof listeners.load === "function", "sinal de vida após carregar");
document.visibilityState = "hidden";
docListeners.visibilitychange();
document.visibilityState = "visible";
docListeners.visibilitychange();
const after = calls.filter(([c]) => c === "service_status").length;
check(after === 3, `sinal de vida ao voltar a ficar visível: ${after}`);

// Mídia em uso: o Papeo não recarrega o serviço
const lastMedia = () => calls.filter(([c]) => c === "service_status").at(-1)[1].media;
await navigator.mediaDevices.getUserMedia({ audio: true });
timers[0]();
check(lastMedia() === true, "microfone em uso conta como mídia");
micLive = false;
timers[0]();
check(lastMedia() === false, "microfone encerrado deixa de contar");

// Mensagem de voz do WhatsApp no WebKit: sem cancelamento de eco (o
// processamento de voz do macOS atrasa ~1 s o começo da gravação).
const button = (label, icon, inFooter = true) => {
  const el = {
    getAttribute: () => label,
    querySelector: () => (icon ? { getAttribute: () => icon } : null),
    closest: (sel) => (sel === "footer" ? (inFooter ? {} : null) : el),
  };
  return { target: el };
};
check(typeof docListeners.pointerdown === "function", "botão de gravar é percebido");
await navigator.mediaDevices.getUserMedia({ audio: true });
check(asked.audio === true, "sem clicar em gravar, o pedido fica igual");
docListeners.pointerdown(button("Mensagem de voz", "ptt"));
await navigator.mediaDevices.getUserMedia({ audio: { deviceId: "x" } });
check(asked.audio.echoCancellation === false && asked.audio.deviceId === "x", "gravar áudio: sem cancelamento de eco");
await navigator.mediaDevices.getUserMedia({ audio: true });
check(asked.audio === true, "só o pedido logo após o clique muda");
// O microfone abre já no toque e é o que o WhatsApp recebe.
micLive = true;
requests = [];
docListeners.pointerdown(button("Mensagem de voz", "ptt"));
check(requests.length === 1 && requests[0].audio.echoCancellation === false, "toque já abre o microfone");
const early = await navigator.mediaDevices.getUserMedia({ audio: true });
check(requests.length === 1 && early.track.stopped === false, "pedido do WhatsApp usa o microfone já aberto");
// Outro aparelho pedido: o aberto no toque é fechado e vem o pedido.
requests = [];
docListeners.pointerdown(button("Mensagem de voz", "ptt"));
const otherMic = await navigator.mediaDevices.getUserMedia({ audio: { deviceId: { exact: "usb" } } });
check(requests.length === 2 && otherMic.track.getSettings().deviceId === "usb", "outro microfone pedido é respeitado");
// Toque no botão e depois em outra coisa: o microfone aberto fecha.
requests = [];
docListeners.pointerdown(button("Mensagem de voz", "ptt"));
docListeners.pointerdown(button("Anexar", "plus"));
await new Promise((r) => setTimeout(r, 0));
check(requests.length === 1, "toque em outro botão não pede microfone");
early.track.stop();
otherMic.track.stop();
docListeners.pointerdown(button("Chamada de voz", "voice-call"));
await navigator.mediaDevices.getUserMedia({ audio: true });
check(asked.audio === true, "chamada mantém o cancelamento de eco");
docListeners.pointerdown(button("Mensagem de voz", "ptt", false));
await navigator.mediaDevices.getUserMedia({ audio: true });
check(asked.audio === true, "botão fora do rodapé não conta");
docListeners.pointerdown(button("Mensagem de voz", "ptt"));
await navigator.mediaDevices.getUserMedia({ audio: true, video: true });
check(asked.audio === true, "vídeo mantém o cancelamento de eco");
window.__papeoVoice = "clean";
docListeners.pointerdown(button("Mensagem de voz", "ptt"));
await navigator.mediaDevices.getUserMedia({ audio: true });
check(asked.audio === true, "com redução de eco e ruído escolhida, o pedido fica igual");
let warmed = 0;
window.__papeoVoice = "enhanced";
window.__papeoVoiceWarmup = () => warmed++;
window.__papeoEnhanceVoice = async (stream) => ({ enhanced: stream, getTracks: () => [] });
docListeners.pointerdown(button("Mensagem de voz", "ptt"));
check(warmed === 1, "limpeza do Papeo é preparada no toque");
const cleaned = await navigator.mediaDevices.getUserMedia({ audio: true });
check(cleaned.enhanced !== undefined, "limpeza do Papeo trata o áudio");
check(
  asked.audio.echoCancellation === false && asked.audio.noiseSuppression === false && asked.audio.autoGainControl === false,
  "limpeza do Papeo pede o microfone cru",
);
const plain = await navigator.mediaDevices.getUserMedia({ audio: true });
check(plain.enhanced === undefined && asked.audio === true, "sem toque, nada de limpeza");
window.__papeoEnhanceVoice = async () => {
  throw new Error("falhou");
};
requests = [];
docListeners.pointerdown(button("Mensagem de voz", "ptt"));
const fallback = await navigator.mediaDevices.getUserMedia({ audio: true });
check(fallback.enhanced === undefined && typeof fallback.getTracks === "function", "falha na limpeza grava assim mesmo");
check(requests.length === 2 && requests[0].audio.noiseSuppression === false, "falha na limpeza pede o microfone de novo");
check(
  requests[1].audio.echoCancellation === false && requests[1].audio.noiseSuppression === undefined,
  "falha na limpeza grava como a instantânea, não com o microfone cru",
);
window.__papeoEnhanceVoice = async (stream) => stream;
requests = [];
docListeners.pointerdown(button("Mensagem de voz", "ptt"));
const again = await navigator.mediaDevices.getUserMedia({ audio: true });
check(requests.length === 2 && again.track.stopped === false, "limpeza que desiste troca o microfone cru pelo normal");
delete window.__papeoEnhanceVoice;
delete window.__papeoVoiceWarmup;
delete window.__papeoVoice;
micLive = false;
const gif = new FakeMedia();
gif.muted = true;
gif.volume = 1;
await gif.play();
timers[0]();
check(lastMedia() === false, "vídeo mudo (GIF) não conta");
const voice = new FakeMedia();
voice.muted = false;
voice.volume = 1;
await voice.play();
timers[0]();
check(lastMedia() === true, "áudio fora da página conta");
voice.paused = true;
timers[0]();
check(lastMedia() === false, "áudio pausado deixa de contar");

// Conversa aberta no cabeçalho (painel do contato)
const lastChat = () => calls.filter(([c]) => c === "service_status").at(-1)[1].chat;
check(lastChat() === "", `sem conversa aberta: ${JSON.stringify(lastChat())}`);
const header = '#main header span[title], #main header span[dir="auto"]';
dom[header] = { getAttribute: (a) => (a === "title" ? "  Ana   Souza " : null) };
timers[0]();
check(lastChat() === "Ana Souza", `conversa aberta: ${JSON.stringify(lastChat())}`);
check(typeof docListeners.click === "function", "troca de conversa por clique é percebida");
delete dom[header];

// Abrir conversa pedida pelo Papeo (nome exato na lista)
check(typeof window.__papeoOpenChat === "function", "abrir conversa disponível");
try {
  window.__papeoOpenChat = null;
} catch {
  /* modo estrito: a atribuição falha, como deve */
}
check(typeof window.__papeoOpenChat === "function", "abrir conversa não pode ser trocada pelo site");
window.__papeoOpenChat("Ana Souza", true);
check(pressed.join() === "mousedown,mouseup,click", `abre a conversa exata: ${pressed}`);
window.__papeoOpenChat("   ", true);
check(pressed.length === 3, "nome vazio não faz nada");

// Resposta rápida: conversa certa no cabeçalho, escreve e envia.
const replyResult = async (id, tries = 40) => {
  for (let i = 0; i < tries; i++) {
    const s = window.__papeoReplyStatus(id);
    if (s !== "pending") return s;
    await new Promise((r) => setTimeout(r, 100));
  }
  return "timeout";
};
check(window.__papeoReplyStatus("nada") === "unknown", "status desconhecido");
window.__papeoReply(1, "Ana Souza", "Oi\nTudo bem?");
check((await replyResult(1)) === "ok", "resposta enviada");
composer.innerText = "rascunho meu";
window.__papeoReply(2, "Ana Souza", "Oi");
check((await replyResult(2)) === "draft", "não envia por cima de rascunho");
check(composer.innerText === "rascunho meu", "rascunho intacto");
// Botão que não envia: o texto escrito pelo Papeo é desfeito.
composer.innerText = "";
const realDispatch = sendButton.dispatchEvent;
sendButton.dispatchEvent = () => true;
window.__papeoReply(4, "Ana Souza", "Oi", 30000);
check((await replyResult(4, 120)) === "send", "não enviado");
check(composer.innerText === "", "texto do Papeo desfeito");
sendButton.dispatchEvent = realDispatch;
// Prazo vencido: nada é escrito.
window.__papeoReply(5, "Ana Souza", "Oi", 1000);
window.__papeoReplyCancel(5);
check((await replyResult(5)) === "timeout", "cancelado não envia");
check(composer.innerText === "", "cancelado não escreve");
// Duas conversas com o mesmo nome: recusa.
chatTitles.push({ getAttribute: () => "Ana Souza", dispatchEvent: () => {} });
window.__papeoReply(6, "Ana Souza", "Oi");
check((await replyResult(6)) === "ambiguous", "nome repetido");
chatTitles.pop();
window.__papeoReply(3, "Ana Souza", "   ");
check((await replyResult(3)) === "empty", "texto vazio");

// Links externos: endereço direto passa intacto; janela em branco repassa o
// endereço definido depois (padrão do WhatsApp).
window.open("https://a.example/", "_blank", "noopener");
check(JSON.stringify(opened.at(-1)) === '["https://a.example/","_blank","noopener"]', "abertura direta intacta");
const blank = window.open();
blank.opener = null;
blank.location = "https://b.example/";
blank.location.href = "https://c.example/";
check(opened.length === 2 && opened[1][0] === "https://b.example/", `janela em branco abre o endereço: ${JSON.stringify(opened)}`);
window.open("about:blank", "_blank").location.href = "https://d.example/";
check(opened.at(-1)[0] === "https://d.example/", "about:blank com location.href");
window.open("", "_blank").location.replace("https://e.example/");
check(opened.at(-1)[0] === "https://e.example/", "location.replace");
const unused = window.open("");
unused.close();
check(opened.length === 4 && unused.closed, "janela em branco fechada não abre nada");

// Clique em link externo cancelado pelo site (WhatsApp): o Papeo abre uma vez.
const clickOn = (href, { target = "_blank", prevented = true, trusted = true } = {}) =>
  listeners.click({
    button: 0,
    isTrusted: trusted,
    defaultPrevented: prevented,
    target: { closest: () => ({ href, target }) },
  });
const tick = () => new Promise((r) => setTimeout(r, 5));
let before = opened.length;
clickOn("https://app.example/agenda");
await tick();
check(opened.length === before + 1 && opened.at(-1)[0] === "https://app.example/agenda", "clique cancelado abre o link");
clickOn("https://app.example/agenda", { trusted: false });
await tick();
check(opened.length === before + 1, "clique invisível do site logo depois não abre de novo");
before = opened.length;
clickOn("https://other.example/", { prevented: false });
await tick();
check(opened.length === before, "clique não cancelado segue o caminho normal");
clickOn("https://wa.me/5585999999999");
clickOn("https://chat.whatsapp.com/convite");
await tick();
check(opened.length === before, "links do próprio WhatsApp ficam com ele");
clickOn("https://other.example/", { target: "" });
await tick();
check(opened.length === before, "link sem nova aba não é aberto pelo Papeo");

// Lista de conversas do WhatsApp: nome e contador de não lidas de cada linha.
const row = (title, label) => ({
  querySelector: (sel) => (sel === "span[title]" ? { getAttribute: () => title } : null),
  querySelectorAll: () => (label ? [{ getAttribute: () => label }] : [{ getAttribute: () => "Conversa fixada" }]),
});
dom["#pane-side"] = {
  querySelectorAll: () => [
    row("Ana  Souza", "3 mensagens não lidas"),
    row("Bruno", null),
    row("Grupo", "1.234 mensagens não lidas"),
    row("", "2 mensagens não lidas"),
  ],
};
timers[0]();
const sentList = calls.filter(([c]) => c === "service_status").at(-1)[1].list;
check(
  JSON.stringify(sentList) === '[["Ana Souza",3],["Bruno",0],["Grupo",1234]]',
  `lista de conversas: ${JSON.stringify(sentList)}`,
);
const beforeTick = calls.length;
timers[1]();
check(calls.length === beforeTick, "lista igual: não envia de novo");
dom["#pane-side"] = { querySelectorAll: () => [row("Bruno", "1 mensagem não lida")] };
timers[1]();
check(calls.length === beforeTick + 1, "lista mudou: envia");
delete dom["#pane-side"];

if (failures.length) {
  console.error("FALHOU:", failures.join("; "));
  process.exit(1);
}
console.log("shim ok");
process.exit(0);
