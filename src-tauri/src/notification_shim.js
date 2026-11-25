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
  // navegador padrão. Alguns sites (o WhatsApp) abrem antes uma janela em
  // branco e só depois definem o endereço; o Papeo veria só "about:blank" e o
  // link não abriria. Aqui a janela em branco é de mentira e, quando o site
  // define o endereço, a abertura vai com ele.
  const nativeOpen = window.open;
  const isBlank = (url) => url === undefined || url === null || String(url) === "" || String(url) === "about:blank";
  window.open = function (url, target, features) {
    if (!isBlank(url) || typeof nativeOpen !== "function") return nativeOpen?.call(window, url, target, features) ?? null;
    let sent = false;
    const go = (next) => {
      if (sent || isBlank(next)) return;
      sent = true;
      nativeOpen.call(window, String(next), "_blank", "noopener");
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

  // Tela de login/QR visível = sessão desconectada (só mensageiros conhecidos).
  const LOGGED_OUT_SELECTORS = {
    "web.whatsapp.com": '[data-ref] canvas, canvas[aria-label*="QR" i], canvas[aria-label*="scan" i]',
    "web.telegram.org": "#auth-qr-form, #auth-phone-number-form, #auth-code-form, #auth-password-form",
    "discord.com": '[class*="authBox"]',
  };
  const loggedOut = () => {
    const selector = LOGGED_OUT_SELECTORS[location.hostname];
    return Boolean(selector && document.querySelector(selector));
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
  const heartbeat = () => {
    lastChat = currentChat();
    invoke("service_status", { loggedOut: loggedOut(), media: mediaActive(), chat: lastChat ?? null });
  };
  setInterval(heartbeat, 20000);
  // Trocou de conversa (clique ou teclado): avisa logo.
  const chatChanged = () =>
    setTimeout(() => {
      const chat = currentChat();
      if (chat !== undefined && chat !== lastChat) heartbeat();
    }, 400);
  document.addEventListener("click", chatChanged, true);
  document.addEventListener("keyup", (e) => e.key === "Enter" || e.key === "Escape" ? chatChanged() : undefined, true);
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
  const openChat = (query, exact) => {
    const name = String(query || "").trim();
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
