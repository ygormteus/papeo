// Diagnóstico dos sites (spec 14) no script injetado: `pnpm test`.
// Confere que cada site relata só presença (true/false/null) dos pontos de
// apoio, depois do aquecimento e fora da tela de login.
import { readFileSync } from "node:fs";
import vm from "node:vm";

const source = readFileSync(new URL("../src-tauri/src/notification_shim.js", import.meta.url), "utf8");
const failures = [];
const check = (ok, msg) => ok || failures.push(msg);

const el = (props = {}) => ({
  textContent: "",
  getAttribute: (name) => props.attrs?.[name] ?? null,
  getClientRects: () => (props.hidden ? [] : [{}]),
  closest: () => null,
  querySelector: () => null,
  ...props,
});

/** Carrega o script com relógio próprio; `status()` roda o sinal de vida. */
function load({ hostname, pathname = "/", hash = "", dom = {}, all = {}, globals = {}, readyState = "complete" }) {
  let clock = 1_000_000;
  class FakeDate extends Date {
    static now() {
      return clock;
    }
  }
  const calls = [];
  const timers = [];
  const location = { hostname, pathname, hash };
  const document = {
    visibilityState: "visible",
    readyState,
    querySelector: (sel) => dom[sel] ?? null,
    querySelectorAll: (sel) => all[sel] ?? (dom[sel] ? [dom[sel]] : []),
    addEventListener() {},
  };
  const ctx = {
    location,
    document,
    navigator: {},
    console,
    Date: FakeDate,
    setInterval: (fn) => timers.push(fn),
    setTimeout: () => 0,
    clearTimeout() {},
    queueMicrotask,
    Promise,
    EventTarget,
    Event,
    HTMLInputElement: class {},
    addEventListener() {},
    __TAURI_INTERNALS__: { invoke: (cmd, args) => (calls.push([cmd, args]), Promise.resolve()) },
    ...globals,
  };
  ctx.window = ctx;
  ctx.open = () => null;
  vm.createContext(ctx);
  vm.runInContext(source, ctx);
  const probes = () => {
    timers[0]();
    return calls.filter(([c]) => c === "service_status").at(-1)[1].probes;
  };
  const wait = (ms) => (clock += ms);
  return { ctx, location, document, dom, probes, wait };
}

const onlyFlags = (p) => Object.values(p).every((v) => v === true || v === false || v === null);

// WhatsApp: nada nos primeiros 30 s; depois lista e busca; cabeçalho e
// campo de texto só com uma conversa aberta.
{
  const dom = {
    "#pane-side span[title]": el(),
    '#side [contenteditable="true"][role="textbox"]': el(),
  };
  const page = load({ hostname: "web.whatsapp.com", dom });
  check(page.probes() === null, "whatsapp: nada durante o aquecimento");
  page.wait(30_000);
  let p = page.probes();
  check(p?.list === true && p.search === true, `whatsapp: lista e busca (${JSON.stringify(p)})`);
  check(p.header === null && p.composer === null, "whatsapp: sem conversa aberta, não avaliado");
  dom["#main"] = el();
  dom['#main header span[title], #main header span[dir="auto"]'] = el();
  p = page.probes();
  check(p.header === true && p.composer === false, `whatsapp: campo de texto sumiu (${JSON.stringify(p)})`);
  check(onlyFlags(p), "whatsapp: só booleanos");
  // Tela do QR: desconectado, não avalia.
  dom['[data-ref] canvas, canvas[aria-label*="QR" i], canvas[aria-label*="scan" i]'] = el();
  check(page.probes() === null, "whatsapp: desconectado não avalia");
}

// Página ainda carregando: não avalia.
{
  const page = load({ hostname: "web.whatsapp.com", readyState: "interactive" });
  page.wait(60_000);
  check(page.probes() === null, "carregando: não avalia");
}

// WhatsApp na aba Status: sem a lista, a busca não é julgada.
{
  const page = load({ hostname: "web.whatsapp.com" });
  page.wait(30_000);
  const p = page.probes();
  check(p.list === false && p.search === null, `whatsapp status (${JSON.stringify(p)})`);
}

// Telegram: conversa aberta pelo endereço (#…); campo de texto só com o
// cabeçalho à vista; canal sem campo relata ausência (o app decide).
{
  const header = ".MiddleHeader .fullName, .chat-info .peer-title, .ChatInfo .fullName";
  const page = load({ hostname: "web.telegram.org", hash: "#12345", dom: { "#editable-message-text": el() } });
  page.wait(30_000);
  let p = page.probes();
  check(p.header === false && p.composer === null && p.search === null && p.list === false, `telegram sem cabeçalho (${JSON.stringify(p)})`);
  page.dom[header] = el();
  check(page.probes().composer === true, "telegram: campo de texto encontrado");
  delete page.dom["#editable-message-text"];
  check(page.probes().composer === false, "telegram: canal sem campo de texto");
  page.location.hash = "";
  p = page.probes();
  check(p.header === null && p.composer === null, "telegram: sem conversa aberta");
}

// Gmail: caixa de entrada; assunto só com um e-mail aberto.
{
  const page = load({ hostname: "mail.google.com", dom: { 'a[href$="#inbox"]': el() } });
  page.wait(30_000);
  let p = page.probes();
  check(p.inbox === true && p.subject === null, `gmail: lista (${JSON.stringify(p)})`);
  page.location.hash = "#inbox/FMfcgzQXJWDsKbwjCLmkLVkMHPjJcfdG";
  p = page.probes();
  check(p.subject === false, "gmail: e-mail aberto sem assunto");
  page.ctx.document.querySelectorAll = (sel) => (sel === "h2.hP" ? [el()] : []);
  check(page.probes().subject === true, "gmail: assunto encontrado");
  page.location.hash = "#search/RelatorioFinanceiro2026";
  check(page.probes().subject === null, "gmail: busca sem e-mail aberto");
  page.location.hash = "#label/Newsletters2026Archive";
  check(page.probes().subject === null, "gmail: etiqueta sem e-mail aberto");
  page.location.hash = "#search/relatorio/FMfcgzQXJWDsKbwjCLmkLVkMHPjJcfdG";
  check(page.probes().subject === true, "gmail: e-mail aberto a partir da busca");
}

// Outlook: busca e assunto do e-mail aberto (/id/ no endereço).
{
  const page = load({ hostname: "outlook.live.com", pathname: "/mail/0/inbox/id/AQMk", dom: { "#topSearchInput, input[aria-label=\"Search\" i], input[aria-label=\"Pesquisar\" i]": el() } });
  page.wait(30_000);
  let p = page.probes();
  check(p.search === true && p.subject === false && p.inbox === null, `outlook sem pastas (${JSON.stringify(p)})`);
  page.dom['[role="tree"]'] = el();
  check(page.probes().inbox === false, "outlook: pastas à vista sem a Caixa de Entrada");
  page.dom['[contenteditable="true"][role="textbox"]'] = el();
  check(page.probes().subject === null, "outlook: rascunho aberto não julga o assunto");
}

// Roundcube: pela API da própria página.
{
  const rcmail = { env: { task: "mail", unread_counts: { INBOX: 3 } }, gui_objects: {} };
  const page = load({ hostname: "webmail.exemplo.com.br", globals: { rcmail } });
  page.wait(30_000);
  let p = page.probes();
  check(p.inbox === true && p.search === false, `roundcube (${JSON.stringify(p)})`);
  rcmail.env.action = "compose";
  p = page.probes();
  check(p.inbox === null && p.search === null, "roundcube: escrevendo, não avalia");
  rcmail.env.action = "";
  rcmail.env.task = "settings";
  p = page.probes();
  check(p.inbox === null && p.search === null, "roundcube: fora do e-mail, não avalia");
}

// Sites sem pontos conhecidos: nada.
{
  for (const hostname of ["discord.com", "exemplo.com"]) {
    const page = load({ hostname });
    page.wait(30_000);
    check(page.probes() === null, `${hostname}: sem pontos`);
  }
}

if (failures.length) {
  console.error("Falhas:\n- " + failures.join("\n- "));
  process.exit(1);
}
console.log("diagnóstico dos sites ok");
