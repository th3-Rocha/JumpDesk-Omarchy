// Popups de aviso/erro no canto da janela, para qualquer tela do app.

export type ToastKind = "error" | "info";
export type Toast = { id: number; kind: ToastKind; text: string };

let next = 1;
export const toasts = $state<Toast[]>([]);

export function dismiss(id: number) {
  const i = toasts.findIndex((t) => t.id === id);
  if (i >= 0) toasts.splice(i, 1);
}

/** Mostra um popup. Erros ficam até o usuário fechar; avisos somem sozinhos. */
export function toast(text: string, kind: ToastKind = "error") {
  // o mesmo texto de novo (ex.: túnel e RDP avisando a mesma coisa) não empilha
  if (toasts.some((t) => t.text === text)) return;
  const id = next++;
  toasts.push({ id, kind, text });
  // com o portal aberto a faixa tem ~296px: no máximo dois na tela,
  // saindo primeiro o informativo mais antigo
  while (toasts.length > 2) {
    const info = toasts.findIndex((t) => t.kind === "info");
    toasts.splice(info >= 0 ? info : 0, 1);
  }
  if (kind === "info") setTimeout(() => dismiss(id), 4000);
}
