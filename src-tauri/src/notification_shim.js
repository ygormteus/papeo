// Injetado nas webviews dos serviços antes do código do site.
// Troca a API de Notification do navegador por uma que pede ao Papeo uma
// notificação nativa e envia um sinal de vida periódico. O site só alcança
// os comandos `service_notify` e `service_status`.
(() => {
  if (window.__papeoNotifications) return;
  window.__papeoNotifications = true;

  const invoke = (cmd, args) => {
    try {
      window.__TAURI_INTERNALS__?.invoke(cmd, args)?.catch?.(() => {});
    } catch (_) {
      /* sem IPC: ignora em silêncio */
    }
  };

  // Links externos: o Papeo nega janelas novas e manda o endereço para o
  // navegador padrão (ou para a prévia). Dois jeitos de os sites fugirem disso:
  // - abrir uma janela em branco e só depois definir o endereço: o Papeo veria
  //   só "about:blank". Aqui a janela em branco é de mentira e, quando o site
  //   define o endereço, a abertura vai com ele;
  // - o WhatsApp cancela o clique no link e clica num link invisível, que
  //   também sai cancelado no Papeo: nenhuma janela é pedida. Clique num link
  //   externo que termina cancelado sem nada aberto: o Papeo abre o link.
  const nativeOpen = window.open;
  let lastOpen = { url: "", at: 0 };
  const openUrl = (url) => {
    lastOpen = { url: String(url), at: Date.now() };
    return nativeOpen.call(window, String(url), "_blank", "noopener");
  };
  const isBlank = (url) => url === undefined || url === null || String(url) === "" || String(url) === "about:blank";
  window.open = function (url, target, features) {
    if (typeof nativeOpen !== "function") return null;
    if (!isBlank(url)) {
      lastOpen = { url: String(url), at: Date.now() };
      return nativeOpen.call(window, url, target, features);
    }
    let sent = false;
    const go = (next) => {
      if (sent || isBlank(next)) return;
      sent = true;
      openUrl(next);
    };
    const loc = { assign: go, replace: go, reload() {}, toString: () => "about:blank" };
    Object.defineProperty(loc, "href", { get: () => "about:blank", set: go });
    const popup = {
      opener: null,
      closed: false,
      focus() {},
      blur() {},
      close() {
        popup.closed = true;
      },
      postMessage() {},
      document: { title: "", write() {}, writeln() {}, open() {}, close() {} },
    };
    Object.defineProperty(popup, "location", { get: () => loc, set: go });
    return popup;
  };

  // Links que o próprio site trata (abrir conversa, convite de grupo/canal).
  const INTERNAL_HOSTS = /(^|\.)(whatsapp\.com|whatsapp\.net|wa\.me|t\.me|telegram\.me|telegram\.org|discord\.gg|discord\.com|discordapp\.com)$/i;
  const externalLink = (e) => {
    const a = e.target?.closest?.("a[href]");
    if (!a || (a.target || "").toLowerCase() !== "_blank") return null;
    let url;
    try {
      url = new URL(a.href, location.href);
    } catch (_) {
      return null;
    }
    if (!/^https?:$/.test(url.protocol) || url.origin === location.origin || INTERNAL_HOSTS.test(url.hostname)) return null;
    return url.href;
  };
  window.addEventListener(
    "click",
    (e) => {
      const href = e.button === 0 ? externalLink(e) : null;
      if (!href) return;
      // Depois de o clique passar por todos (o site pode cancelá-lo).
      setTimeout(() => {
        if (!e.defaultPrevented) return; // segue o caminho normal (janela nova)
        // O clique invisível do site pode vir segundos depois do da pessoa.
        const recent = Date.now() - lastOpen.at < (e.isTrusted ? 2000 : 10000);
        if (lastOpen.url === href && recent) return;
        openUrl(href);
      }, 0);
    },
    true,
  );

  // Site conhecido: o domínio, ou o tipo de webmail (o Roundcube roda no
  // domínio de cada provedor, então é reconhecido pela própria página).
  const site = () => {
    const host = location.hostname;
    if (host === "mail.google.com") return "gmail";
    if (window.rcmail || document.querySelector("#rcmloginuser")) return "roundcube";
    return host;
  };
  const visible = (el) => Boolean(el && el.getClientRects?.().length);
  const firstVisible = (sel, root = document) => [...root.querySelectorAll(sel)].find(visible) || null;

  // Tela de login/QR visível = sessão desconectada (só sites conhecidos).
  const LOGGED_OUT_SELECTORS = {
    "web.whatsapp.com": '[data-ref] canvas, canvas[aria-label*="QR" i], canvas[aria-label*="scan" i]',
    "web.telegram.org": "#auth-qr-form, #auth-phone-number-form, #auth-code-form, #auth-password-form",
    "discord.com": '[class*="authBox"]',
    roundcube: "#rcmloginuser",
  };
  const loggedOut = () => {
    // Gmail desconectado cai nas páginas de login do Google.
    if (location.hostname === "accounts.google.com") {
      return /^\/(v\d\/)?(signin|ServiceLogin|AccountChooser)/i.test(location.pathname || "");
    }
    const selector = LOGGED_OUT_SELECTORS[site()];
    return Boolean(selector && document.querySelector(selector));
  };

  // Caixas de e-mail: o assunto do e-mail aberto faz as vezes da conversa
  // ("" = nenhum aberto) e a página informa as não lidas da caixa de entrada.
  const clean = (text) => String(text || "").replace(/\s+/g, " ").trim().slice(0, 200);
  const roundcubeSubject = () => {
    const own = firstVisible("h2.subject");
    if (own) return own;
    try {
      const frame = document.querySelector("#messagecontframe");
      // Escondido (tela estreita), o quadro ainda guarda o último e-mail.
      const doc = visible(frame) ? frame.contentDocument : null;
      return doc?.querySelector("h2.subject") || null;
    } catch (_) {
      return null; // quadro de outro domínio
    }
  };
  const MAIL_SUBJECT = {
    gmail: () => firstVisible("h2.hP"),
    // Login do Google (Gmail desconectado): nenhum e-mail aberto.
    "accounts.google.com": () => null,
    roundcube: roundcubeSubject,
  };
  // Número com separador de milhar ("1.234", "1 234"); sem número: undefined.
  const digits = (text) => {
    const m = String(text || "").match(/\d[\d.,\u00a0\u202f ]*/);
    return m ? Number(m[0].replace(/\D/g, "")) || 0 : undefined;
  };
  const MAIL_UNREAD = {
    gmail: () => {
      const inbox = document.querySelector('a[href$="#inbox"]');
      if (!inbox) return undefined;
      // Item da caixa de entrada reconhecido: sem o número, nenhuma não lida.
      const item = inbox.closest?.(".aim");
      if (item) return digits(item.querySelector(".bsU")?.textContent) ?? 0;
      // Estrutura desconhecida: só com o número no rótulo; senão vale o título.
      return digits(inbox.getAttribute("aria-label"));
    },
    roundcube: () => {
      const rc = window.rcmail;
      if (!rc || rc.env?.task !== "mail") return undefined;
      const known = rc.env.unread_counts?.INBOX;
      if (typeof known === "number") return known;
      return digits(document.querySelector("#mailboxlist li.inbox .unreadcount")?.textContent) ?? 0;
    },
  };
  const pageUnread = () => {
    try {
      const n = MAIL_UNREAD[site()]?.();
      return Number.isFinite(n) && n >= 0 ? Math.min(Math.floor(n), 999999) : undefined;
    } catch (_) {
      return undefined;
    }
  };

  // Chamada, microfone/câmera, tela compartilhada ou áudio/vídeo com som:
  // o Papeo não recarrega o serviço para liberar memória enquanto isso.
  const captures = new Set();
  const media = navigator.mediaDevices;
  for (const name of ["getUserMedia", "getDisplayMedia"]) {
    if (!media || typeof media[name] !== "function") continue;
    const original = media[name].bind(media);
    media[name] = async (...args) => {
      const stream = await original(...args);
      captures.add(stream);
      return stream;
    };
  }
  // Também os elementos fora da página (`new Audio()`).
  const players = new Set();
  const Media = window.HTMLMediaElement;
  if (Media && typeof Media.prototype.play === "function") {
    const play = Media.prototype.play;
    Media.prototype.play = function (...args) {
      players.add(this);
      return play.apply(this, args);
    };
  }
  // Vídeo mudo em loop (GIF, figurinha) não conta.
  const audible = (m) => m.paused === false && m.ended === false && !m.muted && m.volume > 0;
  const mediaActive = () => {
    try {
      for (const stream of captures) {
        if (stream.getTracks().some((t) => t.readyState === "live")) return true;
        captures.delete(stream);
      }
      for (const m of players) {
        if (audible(m)) return true;
        if (m.paused !== false || m.ended) players.delete(m);
      }
      return Array.from(document.querySelectorAll("audio, video")).some(audible);
    } catch (_) {
      return false;
    }
  };

  // Conversa aberta (nome no cabeçalho), para o painel do contato no
  // Papeo. Só nos mensageiros conhecidos; "" = nenhuma conversa aberta.
  const currentChat = () => {
    const mail = MAIL_SUBJECT[site()];
    if (mail) {
      try {
        const el = mail();
        return el ? clean(el.getAttribute("title") || el.textContent) : "";
      } catch (_) {
        return undefined;
      }
    }
    const sel = CHAT_HEADER[location.hostname];
    if (!sel) return undefined;
    try {
      const el = document.querySelector(sel);
      if (!el) return "";
      return (el.getAttribute("title") || el.textContent || "").replace(/\s+/g, " ").trim().slice(0, 200);
    } catch (_) {
      return undefined;
    }
  };
  let lastChat;

  // Sinal de vida: se o serviço visível parar de enviar, o Papeo oferece recarregar.
  // Lista de conversas do WhatsApp: nome e contador de não lidas de cada
  // linha visível ("3 mensagens não lidas"). Conversa sem contador foi lida,
  // inclusive no celular. Fica só na memória do Papeo.
  const UNREAD_LABEL = /n[aã]o lid|unread|no le[ií]d/i;
  const chatList = () => {
    const pane = document.querySelector("#pane-side");
    if (!pane) return null;
    const out = [];
    for (const row of pane.querySelectorAll('[role="row"]')) {
      const title = (row.querySelector("span[title]")?.getAttribute("title") || "").replace(/\s+/g, " ").trim().slice(0, 200);
      if (!title) continue;
      let unread = 0;
      for (const el of row.querySelectorAll("[aria-label]")) {
        const label = el.getAttribute("aria-label") || "";
        if (UNREAD_LABEL.test(label)) {
          unread = Number((label.match(/\d[\d.]*/)?.[0] || "1").replace(/\./g, "")) || 1;
          break;
        }
      }
      out.push([title, unread]);
      if (out.length >= 60) break;
    }
    return out;
  };
  let lastList = "";
  // Lê a página uma vez só por tique: lista, não lidas e conversa aberta.
  const pageState = () => {
    const chat = currentChat();
    const list = chatList();
    const unread = pageUnread();
    const key = JSON.stringify([list, unread, MAIL_SUBJECT[site()] ? chat : null]);
    return { chat, list, unread, key };
  };
  const report = (page) => {
    lastChat = page.chat;
    lastList = page.key;
    invoke("service_status", {
      loggedOut: loggedOut(),
      media: mediaActive(),
      chat: lastChat ?? null,
      list: page.list,
      unread: page.unread ?? null,
    });
  };
  // Sempre completo: também desfaz não lidas que a página já mostra lidas.
  const heartbeat = () => report(pageState());
  setInterval(heartbeat, 20000);
  // A lista mudou (conversa lida, mensagem nova, e-mail aberto): avisa logo.
  // Vale também oculta: é o que mantém o contador da bandeja em dia.
  setInterval(() => {
    const page = pageState();
    if (page.key !== lastList) report(page);
  }, 5000);
  // Trocou de conversa (clique ou teclado): avisa logo. Cliques seguidos
  // viram uma verificação só.
  let chatTimer = 0;
  const chatChanged = () => {
    clearTimeout(chatTimer);
    chatTimer = setTimeout(() => {
      const chat = currentChat();
      if (chat !== undefined && chat !== lastChat) heartbeat();
    }, 400);
  };
  document.addEventListener("click", chatChanged, true);
  document.addEventListener("keyup", (e) => e.key === "Enter" || e.key === "Escape" ? chatChanged() : undefined, true);
  // O Gmail navega pelo endereço (#inbox/…) e demora um pouco a desenhar.
  window.addEventListener("hashchange", () => {
    chatChanged();
    setTimeout(chatChanged, 1500);
  });
  // Uma mídia começou: avisa logo (no máximo a cada 5 s).
  let lastPlayBeat = 0;
  document.addEventListener(
    "play",
    () => {
      const now = Date.now();
      if (now - lastPlayBeat < 5000) return;
      lastPlayBeat = now;
      heartbeat();
    },
    true,
  );
  window.addEventListener("load", () => setTimeout(heartbeat, 4000));
  document.addEventListener("visibilitychange", () => {
    if (document.visibilityState === "visible") heartbeat();
  });

  // Abrir conversa a pedido do Papeo (barra de menus, busca ⌘K): preenche a
  // busca do próprio site e abre a conversa quando não há dúvida: nome exato
  // (mensagens recentes) ou um único resultado (nome digitado na busca).
  // Só age nos mensageiros conhecidos; nada é enviado de volta ao app.
  const SEARCH_BOX = {
    "web.whatsapp.com": [
      '#side [contenteditable="true"][role="textbox"]',
      '#side input[type="text"]',
      '#side [contenteditable="true"]',
      'input[role="textbox"][aria-label]',
    ],
    "web.telegram.org": ["#telegram-search-input", ".SearchInput input", "input.input-search-input"],
  };
  const CHAT_TITLES = {
    "web.whatsapp.com": "#pane-side span[title]",
    "web.telegram.org": ".chat-list .ListItem .fullName, .search-section .ListItem .fullName, .chatlist .peer-title",
  };
  const findSearchBox = () => {
    for (const sel of SEARCH_BOX[location.hostname] || []) {
      const el = document.querySelector(sel);
      if (el) return el;
    }
    return null;
  };
  const typeInto = (el, text) => {
    el.focus();
    if (el.isContentEditable) {
      document.execCommand("selectAll", false);
      document.execCommand("insertText", false, text);
    } else {
      const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value").set;
      setter.call(el, text);
      el.dispatchEvent(new Event("input", { bubbles: true }));
    }
  };
  // O clique vai no próprio nome: o evento sobe até o elemento da lista que
  // o site escuta (disparado na linha, não desceria até ele).
  const press = (el) => {
    for (const type of ["mousedown", "mouseup", "click"]) {
      el.dispatchEvent(new MouseEvent(type, { bubbles: true, cancelable: true, view: window }));
    }
  };
  const fold = (s) => s.normalize("NFD").replace(/\p{Diacritic}/gu, "").toLowerCase();
  const chatTitles = () => {
    const sel = CHAT_TITLES[location.hostname];
    if (!sel) return [];
    return [...document.querySelectorAll(sel)].map((el) => ({
      el,
      title: (el.getAttribute("title") || el.textContent || "").trim(),
    }));
  };
  // Exato: o nome inteiro. Senão: só se uma única conversa contém o texto.
  const openMatch = (name, exact) => {
    const all = chatTitles();
    let hit;
    if (exact) {
      hit = all.find((c) => c.title === name);
    } else {
      const q = fold(name);
      const found = all.filter((c) => fold(c.title).includes(q));
      if (new Set(found.map((c) => c.title)).size === 1) hit = found[0];
    }
    if (!hit) return false;
    press(hit.el);
    return true;
  };
  // E-mail: busca o texto (assunto ou remetente) na caixa do próprio site e
  // abre o e-mail se a busca trouxer um só, que contenha o texto.
  const MAIL_SEARCH = {
    gmail: {
      ready: () => Boolean(document.querySelector('a[href$="#inbox"]')),
      search: (q) => {
        const phrase = `"${q.replace(/"/g, " ").trim()}"`;
        location.hash = "#search/" + encodeURIComponent(phrase).replace(/%20/g, "+");
      },
      rows: () => [...document.querySelectorAll('div[role="main"] tr.zA')].filter(visible),
    },
    roundcube: {
      ready: () => Boolean(window.rcmail?.gui_objects?.qsearchbox),
      search: (q) => {
        const rc = window.rcmail;
        const box = rc.gui_objects.qsearchbox;
        const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value").set;
        setter.call(box, q);
        rc.command("search", q);
      },
      rows: () => [...document.querySelectorAll("#messagelist tbody tr")].filter(visible),
    },
  };
  const openMail = (provider, name) => {
    let tries = 0;
    const step = () => {
      if (!provider.ready()) {
        if (++tries < 40) setTimeout(step, 500);
        return;
      }
      provider.search(name);
      const q = fold(name);
      let picks = 0;
      const pick = () => {
        const rows = provider.rows();
        if (rows.length === 1 && fold(rows[0].textContent || "").includes(q)) {
          press(rows[0]);
          return;
        }
        if (++picks < 16) setTimeout(pick, 500);
      };
      setTimeout(pick, 1200);
    };
    step();
  };
  const openChat = (query, exact) => {
    const name = String(query || "").trim();
    const mail = MAIL_SEARCH[site()];
    // Assunto longo chega encurtado ("…"): busca só o começo.
    if (name && mail) return openMail(mail, name.replace(/\s*…$/, ""));
    if (!name || !SEARCH_BOX[location.hostname]) return;
    let tries = 0;
    const step = () => {
      if (exact && openMatch(name, true)) return;
      const box = findSearchBox();
      // O site ainda carregando: tenta por até ~20 s.
      if (!box) {
        if (++tries < 40) setTimeout(step, 500);
        return;
      }
      typeInto(box, name);
      // Espera a lista filtrar antes de decidir; desiste em ~4 s.
      let picks = 0;
      const pick = () => {
        if (!openMatch(name, Boolean(exact)) && ++picks < 12) setTimeout(pick, 300);
      };
      setTimeout(pick, 700);
    };
    step();
  };
  Object.defineProperty(window, "__papeoOpenChat", { value: openChat });

  // Resposta rápida (mini janela, notificação): abre a conversa exata,
  // confere o nome no cabeçalho, escreve no campo do próprio site e envia.
  // Nunca envia para outra conversa nem por cima de um rascunho. O Papeo
  // acompanha pelo `__papeoReplyStatus`; nada além disso volta ao app.
  const CHAT_HEADER = {
    "web.whatsapp.com": '#main header span[title], #main header span[dir="auto"]',
    "web.telegram.org": ".MiddleHeader .fullName, .chat-info .peer-title, .ChatInfo .fullName",
  };
  const COMPOSER = {
    "web.whatsapp.com": ['#main footer [contenteditable="true"][role="textbox"]', '#main footer [contenteditable="true"]'],
    "web.telegram.org": ["#editable-message-text", '.input-message-input[contenteditable="true"]'],
  };
  const SEND_BUTTON = {
    "web.whatsapp.com": [
      '#main footer button[aria-label="Send"]',
      '#main footer button[aria-label="Enviar"]',
      '#main footer [data-icon*="send"]',
    ],
    "web.telegram.org": [".Composer button.send", "button.btn-send", ".btn-send"],
  };
  const first = (sels) => {
    for (const sel of sels || []) {
      const el = document.querySelector(sel);
      if (el) return el;
    }
    return null;
  };
  const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
  const waitFor = async (fn, ms) => {
    const end = Date.now() + ms;
    for (;;) {
      const v = fn();
      if (v || Date.now() > end) return v;
      await sleep(250);
    }
  };
  const headerIs = (name) => {
    const sel = CHAT_HEADER[location.hostname];
    if (!sel) return false;
    return [...document.querySelectorAll(sel)].some(
      (el) => (el.getAttribute("title") || el.textContent || "").trim() === name,
    );
  };
  const textOf = (el) => (el.isContentEditable ? el.innerText : el.value || "").trim();
  const insertLines = (el, text) => {
    el.focus();
    text.split("\n").forEach((line, i) => {
      if (i) document.execCommand("insertLineBreak", false);
      if (line) document.execCommand("insertText", false, line);
    });
  };
  const replyStatus = new Map();
  const cancelled = new Set();
  let replyQueue = Promise.resolve();
  const norm = (s) => String(s || "").replace(/\s+/g, " ").trim();
  // Desfaz só o texto que o Papeo escreveu (nunca um rascunho seu).
  const clearOurs = (box, text) => {
    if (norm(textOf(box)) !== norm(text)) return;
    box.focus();
    document.execCommand("selectAll", false);
    document.execCommand("delete", false);
  };
  // Mais de uma conversa com o mesmo nome na lista: não dá para ter certeza.
  // Só antes de buscar: nos resultados da busca, mensagens também trazem o
  // nome da conversa e dariam falso "repetido".
  const ambiguous = (name) => chatTitles().filter((c) => c.title === name).length > 1;
  // Cada pedido tem prazo (contado desde que entrou na fila) e pode ser
  // cancelado pelo Papeo: depois disso nada é escrito nem enviado, e um
  // novo pedido não duplica a mensagem.
  const deliver = async (job) => {
    const { key, name, text, deadline } = job;
    const host = location.hostname;
    const stop = () => cancelled.has(key) || Date.now() > deadline;
    const left = () => Math.max(0, deadline - Date.now());
    if (!COMPOSER[host]) return "unsupported";
    if (stop()) return "timeout";
    if (ambiguous(name)) return "ambiguous";
    if (!headerIs(name)) {
      openChat(name, true);
      if (!(await waitFor(() => headerIs(name), Math.min(15000, left())))) {
        return stop() ? "timeout" : "chat";
      }
    }
    const box = await waitFor(() => first(COMPOSER[host]), Math.min(5000, left()));
    if (!box) return stop() ? "timeout" : "composer";
    if (textOf(box)) return "draft";
    if (stop()) return "timeout";
    insertLines(box, text);
    if (!(await waitFor(() => textOf(box), 2000))) return "type";
    // O botão pode trocar de lugar enquanto o texto entra.
    await sleep(150);
    if (!headerIs(name)) {
      clearOurs(box, text);
      return "chat";
    }
    if (stop()) {
      clearOurs(box, text);
      return "timeout";
    }
    const button = first(SEND_BUTTON[host]);
    if (button) {
      press(button.closest("button") || button);
    } else {
      const k = { key: "Enter", code: "Enter", keyCode: 13, which: 13, bubbles: true, cancelable: true };
      box.dispatchEvent(new KeyboardEvent("keydown", k));
      box.dispatchEvent(new KeyboardEvent("keyup", k));
    }
    const current = () => first(COMPOSER[host]) || box;
    if (await waitFor(() => !textOf(current()), 8000)) return "ok";
    // O texto continua no campo: não foi enviado. Limpa para não virar
    // "rascunho" e travar a próxima resposta.
    if (norm(textOf(current())) === norm(text)) {
      clearOurs(current(), text);
      return "send";
    }
    return "unsure";
  };
  const reply = (id, chat, text, timeoutMs) => {
    const key = String(id);
    const name = String(chat || "").trim();
    const body = String(text || "").trim();
    if (!name || !body) {
      replyStatus.set(key, "empty");
      return;
    }
    const ms = Math.min(Math.max(Number(timeoutMs) || 30000, 1000), 60000);
    const job = { key, name, text: body, deadline: Date.now() + ms };
    replyStatus.set(key, "pending");
    replyQueue = replyQueue
      .then(() => deliver(job))
      .catch(() => "error")
      .then((result) => {
        replyStatus.set(key, result);
        cancelled.delete(key);
        // Não acumula: o Papeo lê o resultado em segundos.
        setTimeout(() => replyStatus.delete(key), 120000);
      });
  };
  Object.defineProperty(window, "__papeoReply", { value: reply });
  Object.defineProperty(window, "__papeoReplyStatus", {
    value: (id) => replyStatus.get(String(id)) || "unknown",
  });
  Object.defineProperty(window, "__papeoReplyCancel", {
    value: (id) => {
      if (replyStatus.get(String(id)) === "pending") cancelled.add(String(id));
    },
  });

  const send = (title, options) => {
    const o = options || {};
    invoke("service_notify", {
      title: title == null ? "" : String(title),
      body: o.body == null ? "" : String(o.body),
      silent: Boolean(o.silent),
    });
  };

  class PapeoNotification extends EventTarget {
    constructor(title, options) {
      super();
      options = options ?? {};
      this.title = String(title ?? "");
      this.body = options.body ?? "";
      this.tag = options.tag ?? "";
      this.icon = options.icon ?? "";
      this.data = options.data ?? null;
      this.silent = Boolean(options.silent);
      this.onclick = null;
      this.onclose = null;
      this.onerror = null;
      this.onshow = null;
      send(this.title, options);
      queueMicrotask(() => this.#fire("show"));
    }

    #fire(type) {
      const event = new Event(type);
      this.dispatchEvent(event);
      this[`on${type}`]?.call(this, event);
    }

    close() {
      this.#fire("close");
    }

    static get permission() {
      return "granted";
    }

    static requestPermission(callback) {
      callback?.("granted");
      return Promise.resolve("granted");
    }

    static get maxActions() {
      return 0;
    }
  }

  Object.defineProperty(window, "Notification", {
    value: PapeoNotification,
    writable: true,
    configurable: true,
  });

  const permissions = navigator.permissions;
  if (permissions?.query) {
    const query = permissions.query.bind(permissions);
    permissions.query = (descriptor) =>
      descriptor?.name === "notifications"
        ? Promise.resolve(
            Object.assign(new EventTarget(), { name: "notifications", state: "granted", onchange: null }),
          )
        : query(descriptor);
  }

  if (window.ServiceWorkerRegistration?.prototype) {
    window.ServiceWorkerRegistration.prototype.showNotification = function (title, options) {
      send(title, options);
      return Promise.resolve();
    };
    window.ServiceWorkerRegistration.prototype.getNotifications = () => Promise.resolve([]);
  }
})();
