// Testa o script injetado nos serviços (notificações e sinal de vida): `pnpm test`.
import { readFileSync } from "node:fs";

const source = readFileSync(new URL("../src-tauri/src/notification_shim.js", import.meta.url), "utf8");
const calls = [];
const timers = [];
const listeners = {};
let qrVisible = false;

globalThis.window = globalThis;
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
Object.defineProperty(globalThis, "navigator", {
  value: {
    permissions: { query: async (d) => ({ state: "prompt", d }) },
    mediaDevices: {
      getUserMedia: async () => ({ getTracks: () => [{ readyState: micLive ? "live" : "ended" }] }),
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
check(timers.length === 1, "um único timer de sinal de vida");
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

if (failures.length) {
  console.error("FALHOU:", failures.join("; "));
  process.exit(1);
}
console.log("shim ok");
process.exit(0);
