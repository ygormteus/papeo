// Testa a parte de e-mail do script injetado (Gmail, Outlook e Roundcube): `pnpm test`.
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
  dispatchEvent(e) {
    (this.events ??= []).push(e.type);
  },
  ...props,
});

/** Carrega o script num "navegador" falso com a página dada. */
function load({ hostname, pathname = "/", dom = {}, all = {}, globals = {} }) {
  const calls = [];
  const timers = [];
  const timeouts = [];
  const winListeners = {};
  const location = { hostname, pathname, hash: "" };
  const document = {
    visibilityState: "visible",
    querySelector: (sel) => dom[sel] ?? null,
    querySelectorAll: (sel) => all[sel] ?? [],
    addEventListener() {},
    execCommand() {},
  };
  const ctx = {
    location,
    document,
    navigator: {},
    console,
    setInterval: (fn) => timers.push(fn),
    setTimeout: (fn) => timeouts.push(fn),
    queueMicrotask,
    Promise,
    EventTarget,
    Event,
    HTMLInputElement: class {},
    MouseEvent: class {
      constructor(type) {
        this.type = type;
      }
    },
    addEventListener: (type, fn) => (winListeners[type] = fn),
    __TAURI_INTERNALS__: { invoke: (cmd, args) => (calls.push([cmd, args]), Promise.resolve()) },
    ...globals,
  };
  ctx.window = ctx;
  ctx.open = () => null;
  vm.createContext(ctx);
  vm.runInContext(source, ctx);
  const status = () => {
    timers[0]();
    return calls.filter(([c]) => c === "service_status").at(-1)[1];
  };
  const flush = () => {
    for (let i = 0; i < 50 && timeouts.length; i++) timeouts.shift()();
  };
  return { ctx, location, status, flush, timers, winListeners };
}

// Gmail: assunto aberto, não lidas da caixa de entrada (não do título).
{
  const badge = el({ textContent: "1.234" });
  const inbox = el({
    attrs: { "aria-label": "Caixa de entrada 1234 não lidas" },
    closest: (sel) => (sel === ".aim" ? { querySelector: (s) => (s === ".bsU" ? badge : null) } : null),
  });
  const subject = el({ textContent: "  Proposta   de orçamento " });
  const hiddenSubject = el({ textContent: "Antigo", hidden: true });
  const page = load({
    hostname: "mail.google.com",
    dom: { 'a[href$="#inbox"]': inbox },
    all: { "h2.hP": [hiddenSubject, subject] },
  });
  let s = page.status();
  check(s.chat === "Proposta de orçamento", `gmail: assunto visível (${s.chat})`);
  check(s.unread === 1234, `gmail: não lidas da página (${s.unread})`);
  check(s.loggedOut === false, "gmail: conectado");
  check(typeof page.winListeners.hashchange === "function", "gmail: acompanha a navegação");

  // Sem e-mail aberto: "" (lista), e sem o contador: 0.
  page.ctx.document.querySelectorAll = () => [];
  badge.textContent = "";
  s = page.status();
  check(s.chat === "" && s.unread === 0, "gmail: caixa de entrada sem e-mail aberto");

  // Abrir pela tarefa: busca a frase e só abre com um resultado que a contém.
  const row = el({ textContent: "Fornecedor Proposta de orçamento - segue anexo" });
  const other = el({ textContent: "Outro" });
  let rows = [row, other];
  page.ctx.document.querySelectorAll = (sel) => (sel === 'div[role="main"] tr.zA' ? rows : []);
  page.ctx.__papeoOpenChat("Proposta de orçamento", true);
  page.flush();
  check(page.location.hash === "#search/%22Proposta+de+or%C3%A7amento%22", `gmail: busca (${page.location.hash})`);
  check(!row.events, "gmail: dois resultados, não abre nenhum");
  rows = [row];
  page.ctx.__papeoOpenChat("Proposta de orçamento", true);
  page.flush();
  check(row.events?.includes("click"), "gmail: um resultado, abre");
  const stranger = el({ textContent: "Sem relação" });
  rows = [stranger];
  page.ctx.__papeoOpenChat("Proposta", true);
  page.flush();
  check(!stranger.events, "gmail: resultado sem o texto não abre");
}

// Gmail com estrutura desconhecida: só vale o número do rótulo; sem ele,
// fica o do título (undefined). Item conhecido sem número: zero.
{
  const bare = (label) =>
    load({ hostname: "mail.google.com", dom: { 'a[href$="#inbox"]': el({ attrs: { "aria-label": label } }) } }).status();
  check(bare("Inbox").unread === null, "gmail: sem número e sem estrutura, vale o título");
  check(bare("Inbox 1 234 unread").unread === 1234, "gmail: número no rótulo");
  const known = el({ closest: (sel) => (sel === ".aim" ? { querySelector: () => null } : null) });
  check(load({ hostname: "mail.google.com", dom: { 'a[href$="#inbox"]': known } }).status().unread === 0, "gmail: caixa em dia");
}

// Gmail desconectado: páginas de login do Google.
{
  check(load({ hostname: "accounts.google.com", pathname: "/v3/signin/identifier" }).status().chat === "", "google: sem e-mail aberto");
  check(load({ hostname: "accounts.google.com", pathname: "/v3/signin/identifier" }).status().loggedOut, "google: login");
  check(load({ hostname: "accounts.google.com", pathname: "/ServiceLogin" }).status().loggedOut, "google: ServiceLogin");
  check(!load({ hostname: "accounts.google.com", pathname: "/b/0/ManageAccount" }).status().loggedOut, "google: conta");
}

// Roundcube (qualquer domínio): contador do próprio Roundcube, assunto no quadro de leitura.
{
  const searches = [];
  const qsearchbox = { value: "" };
  const rcmail = {
    env: { task: "mail", unread_counts: { INBOX: 7 } },
    gui_objects: { qsearchbox },
    command: (cmd, q) => searches.push([cmd, q]),
  };
  const frameSubject = el({ textContent: "Boleto de novembro" });
  const page = load({
    hostname: "webmail.exemplo.com.br",
    dom: { "#messagecontframe": { getClientRects: () => [{}], contentDocument: { querySelector: (s) => (s === "h2.subject" ? frameSubject : null) } } },
    globals: { rcmail },
  });
  Object.defineProperty(page.ctx.HTMLInputElement.prototype, "value", {
    set(v) {
      this._v = v;
    },
    configurable: true,
  });
  let s = page.status();
  check(s.unread === 7 && s.chat === "Boleto de novembro", `roundcube: ${JSON.stringify(s)}`);
  rcmail.env.unread_counts = {};
  page.ctx.document.querySelector = (sel) =>
    sel === "#mailboxlist li.inbox .unreadcount" ? { textContent: "(3)" } : null;
  s = page.status();
  check(s.unread === 3 && s.chat === "", `roundcube: contador da lista de pastas (${JSON.stringify(s)})`);
  const row = el({ textContent: "Banco Boleto de novembro 12:00" });
  page.ctx.document.querySelectorAll = (sel) => (sel === "#messagelist tbody tr" ? [row] : []);
  page.ctx.__papeoOpenChat("Boleto de novembro", true);
  page.flush();
  check(searches.at(-1)?.[0] === "search" && searches.at(-1)?.[1] === "Boleto de novembro", "roundcube: busca");
  check(qsearchbox._v === "Boleto de novembro", "roundcube: texto na caixa de busca");
  check(row.events?.includes("click"), "roundcube: abre o único resultado");

  // Tela de login do Roundcube.
  const login = load({ hostname: "mail.exemplo.org", dom: { "#rcmloginuser": el() } }).status();
  check(login.loggedOut && login.unread === null, "roundcube: desconectado, sem contador");
}

// Outlook (pessoal e Microsoft 365): número da Caixa de Entrada na lista de
// pastas, assunto no painel de leitura, login da Microsoft = desconectado.
{
  const inbox = el({ textContent: "Caixa de Entrada12", attrs: { title: "Caixa de Entrada" } });
  const subject = el({ textContent: "Reunião de quinta" });
  const page = load({
    hostname: "outlook.live.com",
    pathname: "/mail/0/",
    dom: { '[role="treeitem"][data-folder-name="inbox" i]': inbox },
    all: {
      '#ReadingPaneContainerId [role="heading"], [aria-label="Reading Pane" i] [role="heading"], [aria-label="Painel de Leitura" i] [role="heading"]':
        [subject],
    },
  });
  let s = page.status();
  check(s.unread === 12 && s.chat === "Reunião de quinta", `outlook: ${JSON.stringify(s)}`);
  check(s.loggedOut === false, "outlook: conectado");
  // Pasta sem número: nenhuma não lida.
  inbox.textContent = "Caixa de Entrada";
  s = page.status();
  check(s.unread === 0, `outlook: caixa em dia (${s.unread})`);

  // Sem o atributo da pasta: acha pelo nome entre as pastas.
  const work = load({
    hostname: "outlook.office.com",
    all: { '[role="treeitem"]': [el({ textContent: "Favorites" }), el({ textContent: "Inbox 3" })] },
  }).status();
  check(work.unread === 3 && work.chat === "", `outlook 365: pasta pelo nome (${JSON.stringify(work)})`);
  // Estrutura desconhecida: vale o título.
  check(load({ hostname: "outlook.cloud.microsoft" }).status().unread === null, "outlook: sem pasta, vale o título");

  for (const hostname of ["login.live.com", "login.microsoftonline.com"]) {
    const login = load({ hostname }).status();
    check(login.loggedOut && login.chat === "", `microsoft: login em ${hostname}`);
  }
}

// Outros sites continuam sem contador da página (vale o título).
{
  const s = load({ hostname: "web.whatsapp.com" }).status();
  check(s.unread === null, "whatsapp: contador pelo título");
}

if (failures.length) {
  console.error("Falhas:\n- " + failures.join("\n- "));
  process.exit(1);
}
console.log("e-mail ok");
