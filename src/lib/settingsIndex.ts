// Índice das opções das Configurações para a busca (⌘K). `label` é o texto
// exato do rótulo da linha (ou do título da seção) em Settings.svelte,
// SyncSettings.svelte ou QuickReplies.svelte: é por ele que a tela encontra a
// linha para rolar e destacar. `tests/settings-index.test.mjs` confere isso.

export type SettingsTab = "geral" | "notificacoes" | "privacidade" | "sistema" | "arquivos" | "dados" | "sobre";

export const TAB_LABEL: Record<SettingsTab, string> = {
  geral: "Geral",
  notificacoes: "Notificações",
  privacidade: "Privacidade",
  sistema: "Sistema",
  arquivos: "Arquivos",
  dados: "Dados",
  sobre: "Sobre",
};

export type SettingsTarget = { tab: SettingsTab; label: string };

export type SettingsEntry = SettingsTarget & {
  /** Outras palavras que a pessoa pode digitar. */
  keywords: string;
};

export const SETTINGS_INDEX: SettingsEntry[] = [
  { tab: "geral", label: "Tema", keywords: "aparência claro escuro dark oled preto sistema cores" },
  { tab: "geral", label: "Contas mostradas", keywords: "modo trabalho pessoal expediente horário contas" },
  { tab: "geral", label: "Hibernar as contas fora do modo", keywords: "memória trabalho pessoal dormir" },
  { tab: "geral", label: "Moldura com a cor da conta", keywords: "borda cor conta errada" },
  { tab: "geral", label: "Respostas prontas", keywords: "atalho texto modelo resposta rápida barra" },
  {
    tab: "geral",
    label: "Gravação de mensagens de voz",
    keywords: "áudio audio microfone whatsapp eco ruído atraso gravar voz mudo limpeza isolamento instantânea redução",
  },
  { tab: "notificacoes", label: "Silenciar todas as notificações", keywords: "não perturbe dnd mudo silêncio pausar" },
  { tab: "notificacoes", label: "Horário silencioso todos os dias", keywords: "não perturbe noite dormir agenda" },
  {
    tab: "notificacoes",
    label: "Trocar para a conta da notificação ao voltar",
    keywords: "notificação abrir conta clicar",
  },
  { tab: "privacidade", label: "Prévia nas notificações", keywords: "conteúdo mensagem tela bloqueada remetente oculta" },
  { tab: "privacidade", label: "Bloquear o Papeo", keywords: "senha pin touch id windows hello cadeado trava segurança" },
  {
    tab: "privacidade",
    label: "Esconder em capturas e compartilhamento de tela",
    keywords: "screenshot gravação reunião chamada",
  },
  { tab: "privacidade", label: "Sessões antigas no computador", keywords: "limpar apagar logins dados" },
  { tab: "sistema", label: "Abrir com o sistema", keywords: "iniciar inicialização login autostart" },
  { tab: "sistema", label: "Começar escondido na bandeja", keywords: "minimizado menu bar oculto" },
  { tab: "sistema", label: "Fechar a janela mantém o Papeo na bandeja", keywords: "sair fechar segundo plano" },
  { tab: "sistema", label: "Recarregar serviço pesado", keywords: "memória ram limite gb" },
  { tab: "arquivos", label: "Abrir links", keywords: "navegador prévia link" },
  { tab: "arquivos", label: "Anexos", keywords: "arquivos visualizar pdf planilha quick look" },
  { tab: "sistema", label: "Confirmar antes de liberar memória", keywords: "ram recarregar perguntar" },
  { tab: "sistema", label: "Verificar atualizações", keywords: "versão update atualizar nova" },
  {
    tab: "sistema",
    label: "Modo de diagnóstico",
    keywords: "diagnóstico site mudou quebrado problema resposta rápida não funciona seletores",
  },
  { tab: "arquivos", label: "Pasta dos arquivos baixados", keywords: "downloads pasta salvar" },
  { tab: "dados", label: "Sincronização", keywords: "google drive nuvem outro computador sync" },
  { tab: "dados", label: "Exportar ou restaurar a configuração", keywords: "backup importar exportar copiar" },
  { tab: "sobre", label: "Sobre", keywords: "versão autor github licença código problema bug" },
];

const normalize = (s: string) => s.normalize("NFD").replace(/\p{Diacritic}/gu, "").toLowerCase();

/** Opções que casam com todas as palavras da busca (no rótulo, na aba ou nas palavras-chave). */
export function searchSettings(query: string, limit = 5): SettingsEntry[] {
  const words = normalize(query).split(/\s+/).filter(Boolean);
  if (!words.length) return [];
  const scored = SETTINGS_INDEX.map((e) => {
    const label = normalize(e.label);
    const all = `${label} ${normalize(TAB_LABEL[e.tab])} ${normalize(e.keywords)}`;
    if (!words.every((w) => all.includes(w))) return null;
    // Rótulo que começa com a busca vem primeiro; depois, rótulo que contém.
    const score = label.startsWith(words[0]) ? 0 : words.every((w) => label.includes(w)) ? 1 : 2;
    return { e, score };
  }).filter((x) => x !== null);
  return scored.sort((a, b) => a.score - b.score).slice(0, limit).map((x) => x.e);
}
