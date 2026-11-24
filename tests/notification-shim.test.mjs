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
globalThis.document = {
  visibilityState: "visible",
  querySelector: (sel) => (qrVisible && sel.includes("canvas") ? {} : null),
  addEventListener: (type, fn) => (docListeners[type] = fn),
};
class SWR {}
window.ServiceWorkerRegistration = SWR;
Object.defineProperty(globalThis, "navigator", {
  value: { permissions: { query: async (d) => ({ state: "prompt", d }) } },
  configurable: true,
});

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
check(typeof listeners.load === "function", "sinal de vida após carregar");
document.visibilityState = "hidden";
docListeners.visibilitychange();
document.visibilityState = "visible";
docListeners.visibilitychange();
const after = calls.filter(([c]) => c === "service_status").length;
check(after === 3, `sinal de vida ao voltar a ficar visível: ${after}`);

if (failures.length) {
  console.error("FALHOU:", failures.join("; "));
  process.exit(1);
}
console.log("shim ok");
process.exit(0);
