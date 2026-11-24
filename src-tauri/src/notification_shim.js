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

  // Tela de login/QR visível = sessão desconectada (só mensageiros conhecidos).
  const LOGGED_OUT_SELECTORS = {
    "web.whatsapp.com": '[data-ref] canvas, canvas[aria-label*="QR" i], canvas[aria-label*="scan" i]',
    "web.telegram.org": "#auth-qr-form, #auth-phone-number-form, #auth-code-form, #auth-password-form",
  };
  const loggedOut = () => {
    const selector = LOGGED_OUT_SELECTORS[location.hostname];
    return Boolean(selector && document.querySelector(selector));
  };

  // Sinal de vida: se o serviço visível parar de enviar, o Papeo oferece recarregar.
  const heartbeat = () => invoke("service_status", { loggedOut: loggedOut() });
  setInterval(heartbeat, 20000);
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
