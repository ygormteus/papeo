// Idioma da interface (spec 20). Estilo gettext: a frase em português é a
// chave; `i18n/en/*.ts` traz a versão em inglês. Sem tradução, fica o
// português. `t()` lê o idioma de um `$state`: no template, troca na hora.
// Em código fora do template, chame `t()` na hora de usar (não guarde o
// texto em constante de módulo). `tests/i18n.test.mjs` confere o catálogo.

import { en } from "./i18n/en/index";

export type Lang = "pt" | "en";

/** Idioma do navegador/sistema: português para `pt*`, inglês para o resto. */
export function langOf(tag: string | undefined | null): Lang {
  return /^pt([-_]|$)/i.test(tag?.trim() ?? "") ? "pt" : "en";
}

function initial(): Lang {
  const injected = (globalThis as { __PAPEO_LANG?: string }).__PAPEO_LANG;
  if (injected === "pt" || injected === "en") return injected;
  return langOf(globalThis.navigator?.language);
}

let current = $state<Lang>(initial());

function stamp(lang: Lang) {
  if (typeof document !== "undefined") document.documentElement.lang = lang === "en" ? "en" : "pt-BR";
}
stamp(current);

export const i18n = {
  get lang(): Lang {
    return current;
  },
};

/** Troca o idioma (vem do estado do backend). */
export function setLang(lang: Lang | undefined) {
  if ((lang === "pt" || lang === "en") && lang !== current) {
    current = lang;
    stamp(lang);
  }
}

/** Frase no idioma atual; `{nome}` trocado pelos valores. */
export function t(pt: string, vars?: Record<string, string | number>): string {
  const text = current === "en" ? (en[pt] ?? pt) : pt;
  if (!vars) return text;
  return text.replace(/\{(\w+)\}/g, (m, k: string) => (k in vars ? String(vars[k]) : m));
}

/** Marca uma frase para o catálogo sem traduzir agora (listas de rótulos
 * traduzidas depois com `t(valor)`), como o `N_()` do gettext. */
export const tk = (pt: string): string => pt;

/** Localidade para `toLocale*`/`Intl`. */
export function locale(): string {
  return current === "en" ? "en-US" : "pt-BR";
}
