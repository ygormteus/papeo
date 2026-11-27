// Envios de e-mail em espera para desfazer (spec 16). Estado global: vive
// mesmo com o leitor da conta desmontado (troca de conta durante a espera).
// O backend guarda o e-mail no disco até ele voltar ao editor
// (`mailUnsentTaken`), então um e-mail devolvido não se perde se o app
// fechar antes de a conta abrir.
import { api, onMailSend, type MailDraft } from "./api";

export interface Pending {
  holdId: string;
  /** Conta. */
  id: string;
  subject: string;
  /** Fim da espera (ms, relógio local). */
  until: number;
}

/** E-mail que volta ao editor: desfeito, falhou ou sobrou de outra sessão. */
export interface Returned {
  holdId: string;
  id: string;
  draft: MailDraft;
  notice: string;
  error: boolean;
}

export const outbox = $state({
  pending: [] as Pending[],
  returned: [] as Returned[],
  /** Último envio concluído (para o aviso "enviado"), por conta. */
  sent: null as { id: string; n: number } | null,
});

let started: Promise<unknown> | null = null;
let sentCount = 0;
/** Envios que já terminaram (o resultado pode chegar antes do `push`). */
const finished = new Set<string>();
/** Já foram para o editor nesta sessão: uma leitura do disco atrasada
 * (antes de `mailUnsentTaken` apagar) não os devolve de novo. */
const inEditor = new Set<string>();

function giveBack(r: Returned) {
  if (inEditor.has(r.holdId) || outbox.returned.some((x) => x.holdId === r.holdId)) return;
  outbox.returned.push(r);
}

/** Liga o ouvinte do resultado dos envios (uma vez por janela). */
export function startOutbox() {
  started ??= onMailSend((o) => {
    finished.add(o.holdId);
    outbox.pending = outbox.pending.filter((p) => p.holdId !== o.holdId);
    if (o.error && o.draft) {
      giveBack({ holdId: o.holdId, id: o.id, draft: o.draft, notice: o.error, error: true });
    } else if (!o.error) {
      outbox.sent = { id: o.id, n: ++sentCount };
    }
  });
  return started;
}

/** Confere e agenda; erro de validação sobe para o editor. */
export async function queueSend(id: string, draft: MailDraft) {
  await startOutbox();
  const hold = await api.mailSendLater(id, draft);
  if (finished.has(hold.holdId)) return;
  outbox.pending.push({
    holdId: hold.holdId,
    id,
    subject: draft.subject,
    until: Date.now() + hold.secs * 1000,
  });
}

/** Desfaz; `false` se o e-mail já saiu. */
export async function undoSend(holdId: string): Promise<boolean> {
  const p = outbox.pending.find((x) => x.holdId === holdId);
  const draft = await api.mailSendCancel(holdId);
  if (!draft) return false;
  finished.add(holdId);
  outbox.pending = outbox.pending.filter((x) => x.holdId !== holdId);
  if (p) giveBack({ holdId, id: p.id, draft, notice: "", error: false });
  return true;
}

/** Busca os e-mails guardados da conta que ainda não voltaram ao editor. */
export async function loadUnsent(id: string) {
  const list = await api.mailUnsent(id);
  for (const u of list) {
    giveBack({
      holdId: u.holdId,
      id,
      draft: u.draft,
      notice: u.maybeSent
        ? "O app fechou durante o envio deste e-mail. Confira em Enviados antes de mandar de novo."
        : (u.error ?? "Este e-mail não foi enviado."),
      error: !u.maybeSent && !!u.error,
    });
  }
}

/** Tira o próximo e-mail devolvido da conta, se houver. */
export function takeReturned(id: string): Returned | null {
  const i = outbox.returned.findIndex((r) => r.id === id);
  if (i < 0) return null;
  const [r] = outbox.returned.splice(i, 1);
  inEditor.add(r.holdId);
  return r;
}

/** O e-mail já está no editor: o backend pode apagar a cópia guardada. */
export function releaseReturned(r: Returned) {
  api.mailUnsentTaken(r.holdId).catch(() => {});
}
