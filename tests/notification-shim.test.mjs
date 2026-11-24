// Testa o shim de Notification injetado nos serviços: `pnpm test`.
import { readFileSync } from "node:fs";
const calls = [];
globalThis.window = globalThis;
window.__TAURI_INTERNALS__ = { invoke: (cmd, args) => (calls.push([cmd, args]), Promise.resolve()) };
class SWR {}
window.ServiceWorkerRegistration = SWR;
Object.defineProperty(globalThis, "navigator", { value: { permissions: { query: async (d) => ({ state: "prompt", d }) } }, configurable: true });
eval(readFileSync(new URL("../src-tauri/src/notification_shim.js", import.meta.url), "utf8"));
eval(readFileSync(new URL("../src-tauri/src/notification_shim.js", import.meta.url), "utf8")); // idempotente
let shown = 0;
const n = new Notification("Ana", { body: "Oi!", tag: "x" });
n.onshow = () => shown++;
let closed = 0;
const n2 = new Notification("Sem opções", null);
n2.addEventListener("close", () => closed++);
n2.close();
await Promise.resolve();
await new SWR().showNotification("Grupo", { body: 42, silent: true });
const perm = await Notification.requestPermission();
const q = await navigator.permissions.query({ name: "notifications" });
const other = await navigator.permissions.query({ name: "camera" });
const ok =
  calls.length === 3 && closed === 1 &&
  JSON.stringify(calls[0]) === JSON.stringify(["service_notify", { title: "Ana", body: "Oi!", silent: false }]) &&
  JSON.stringify(calls[1]) === JSON.stringify(["service_notify", { title: "Sem opções", body: "", silent: false }]) &&
  JSON.stringify(calls[2]) === JSON.stringify(["service_notify", { title: "Grupo", body: "42", silent: true }]) &&
  shown === 1 && Notification.permission === "granted" && perm === "granted" && q.state === "granted" && other.state === "prompt";
console.log(ok ? "shim ok" : "FALHOU", JSON.stringify(calls), shown);
process.exit(ok ? 0 : 1);
