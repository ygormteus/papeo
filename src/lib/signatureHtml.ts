// Assinatura formatada (spec 22), lado da interface: limpeza do que é
// colado (num documento inerte, sem rodar nada nem carregar imagens),
// versão em texto e redução de imagens. O backend limpa de novo antes de
// guardar (`papeo_core::signature`); esta lista segue a dele.

const TAGS = new Set([
  "A", "B", "STRONG", "I", "EM", "U", "S", "BR", "P", "DIV", "SPAN", "FONT", "IMG", "UL", "OL", "LI",
  "TABLE", "TBODY", "THEAD", "TR", "TD", "TH", "HR", "SMALL", "BIG", "BLOCKQUOTE", "H1", "H2", "H3", "H4",
  "CENTER", "SUB", "SUP",
]);
/** Somem com o conteúdo (o resto some e deixa o texto de dentro). */
const DROP = new Set(["SCRIPT", "STYLE", "HEAD", "TITLE", "META", "LINK", "IFRAME", "OBJECT", "EMBED", "TEMPLATE", "SVG", "MATH", "FORM", "INPUT", "BUTTON", "SELECT", "TEXTAREA", "NOSCRIPT"]);
const ATTRS = new Set([
  "style", "align", "valign", "width", "height", "dir", "bgcolor", "color", "size", "face", "border",
  "cellpadding", "cellspacing", "colspan", "rowspan",
]);
const STYLES = new Set([
  "color", "background-color", "font-family", "font-size", "font-style", "font-weight", "text-decoration",
  "text-align", "line-height", "letter-spacing", "vertical-align", "white-space", "width", "height",
  "max-width", "min-width", "margin", "margin-top", "margin-right", "margin-bottom", "margin-left", "padding",
  "padding-top", "padding-right", "padding-bottom", "padding-left", "border", "border-top", "border-right",
  "border-bottom", "border-left", "border-color", "border-style", "border-width", "border-radius",
  "border-collapse", "display",
]);
export const IMAGE_TYPES = ["image/png", "image/jpeg", "image/gif", "image/webp"];
/** Teto de cada imagem e da assinatura inteira (o mesmo do backend). */
export const IMAGE_MAX = 200 * 1024;
export const SIGNATURE_MAX = 600 * 1024;
/** Largura máxima de uma imagem guardada. */
export const IMAGE_WIDTH = 600;

const DATA_IMAGE = /^data:image\/(png|jpeg|gif|webp)(;[^,;]*)*;base64,/i;

function safeUrl(value: string, image: boolean): boolean {
  const v = value.trim().toLowerCase();
  if (image) return DATA_IMAGE.test(v) || v.startsWith("https:") || v.startsWith("http:");
  return v.startsWith("https:") || v.startsWith("http:") || v.startsWith("mailto:") || v.startsWith("tel:");
}

function cleanStyle(style: string): string {
  return style
    .split(";")
    .map((d) => d.trim())
    .filter((d) => {
      const [name, ...rest] = d.split(":");
      const value = rest.join(":").toLowerCase();
      return name && STYLES.has(name.trim().toLowerCase()) && !value.includes("url(") && !value.includes("expression");
    })
    .join("; ");
}

function cleanNode(node: Node, doc: Document): Node[] {
  if (node.nodeType === Node.TEXT_NODE) return [doc.createTextNode(node.textContent ?? "")];
  if (node.nodeType !== Node.ELEMENT_NODE) return [];
  const el = node as Element;
  const tag = el.tagName.toUpperCase();
  if (DROP.has(tag)) return [];
  const children = [...el.childNodes].flatMap((c) => cleanNode(c, doc));
  if (!TAGS.has(tag)) return children;
  const out = doc.createElement(tag.toLowerCase());
  for (const attr of [...el.attributes]) {
    const name = attr.name.toLowerCase();
    if (ATTRS.has(name)) {
      const value = name === "style" ? cleanStyle(attr.value) : attr.value;
      if (value) out.setAttribute(name, value);
    } else if (tag === "A" && (name === "href" || name === "title")) {
      if (name === "title" || safeUrl(attr.value, false)) out.setAttribute(name, attr.value);
    } else if (tag === "IMG" && (name === "src" || name === "alt" || name === "title")) {
      if (name !== "src" || safeUrl(attr.value, true)) out.setAttribute(name, attr.value);
    }
  }
  if (tag === "IMG" && !out.getAttribute("src")) return [];
  out.append(...children);
  return [out];
}

/** HTML colado de fora, só com o que a assinatura aceita. */
export function cleanPasted(html: string): string {
  const doc = new DOMParser().parseFromString(html, "text/html");
  const box = doc.createElement("div");
  box.append(...[...doc.body.childNodes].flatMap((n) => cleanNode(n, doc)));
  return box.innerHTML;
}

const BLOCKS = new Set(["P", "DIV", "LI", "TR", "H1", "H2", "H3", "H4", "BLOCKQUOTE", "TABLE", "UL", "OL", "CENTER", "HR"]);

/** Versão em texto da assinatura (a que aparece no editor da mensagem). */
export function signatureText(root: HTMLElement, imageLabel: string): string {
  let out = "";
  const walk = (node: Node) => {
    if (node.nodeType === Node.TEXT_NODE) {
      out += (node.textContent ?? "").replace(/\s+/g, " ");
      return;
    }
    if (node.nodeType !== Node.ELEMENT_NODE) return;
    const el = node as HTMLElement;
    const tag = el.tagName;
    if (tag === "BR") {
      out += "\n";
      return;
    }
    if (tag === "TD" || tag === "TH") out += " ";
    const block = BLOCKS.has(tag);
    if (block && out && !out.endsWith("\n")) out += "\n";
    const start = out.length;
    el.childNodes.forEach(walk);
    if (tag === "A") {
      const href = (el.getAttribute("href") ?? "").replace(/^mailto:|^tel:/i, "");
      const label = out.slice(start).trim();
      if (href && label && label !== href) out += ` <${href}>`;
    }
    if (block && !out.endsWith("\n")) out += "\n";
  };
  root.childNodes.forEach(walk);
  const text = out
    .split("\n")
    .map((l) => l.replace(/[ \t]+/g, " ").trim())
    .join("\n")
    .replace(/\n{3,}/g, "\n\n")
    .trim();
  if (text) return text;
  return root.querySelector("img") ? `[${imageLabel}]` : "";
}

/** Tamanho em bytes de uma imagem `data:`. */
export function dataSize(url: string): number {
  const b64 = url.slice(url.indexOf(",") + 1);
  return Math.floor((b64.length * 3) / 4);
}

function readAsDataUrl(file: Blob): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(String(reader.result));
    reader.onerror = () => reject(reader.error);
    reader.readAsDataURL(file);
  });
}

/** Imagem escolhida, reduzida para caber (largura e tamanho). `null` se
 * não couber nem reduzida. */
export async function prepareImage(file: File): Promise<string | null> {
  if (!IMAGE_TYPES.includes(file.type)) return null;
  const original = await readAsDataUrl(file);
  const bitmap = await createImageBitmap(file);
  const fits = bitmap.width <= IMAGE_WIDTH && dataSize(original) <= IMAGE_MAX;
  // GIF animado e imagens que já cabem vão como estão.
  if (fits) {
    bitmap.close();
    return original;
  }
  const scale = Math.min(1, IMAGE_WIDTH / bitmap.width);
  const canvas = document.createElement("canvas");
  canvas.width = Math.max(1, Math.round(bitmap.width * scale));
  canvas.height = Math.max(1, Math.round(bitmap.height * scale));
  const ctx = canvas.getContext("2d");
  if (!ctx) return null;
  ctx.drawImage(bitmap, 0, 0, canvas.width, canvas.height);
  bitmap.close();
  const png = canvas.toDataURL("image/png");
  if (dataSize(png) <= IMAGE_MAX) return png;
  // Foto grande: JPEG sobre fundo branco, baixando a qualidade.
  const flat = document.createElement("canvas");
  flat.width = canvas.width;
  flat.height = canvas.height;
  const fctx = flat.getContext("2d");
  if (!fctx) return null;
  fctx.fillStyle = "#fff";
  fctx.fillRect(0, 0, flat.width, flat.height);
  fctx.drawImage(canvas, 0, 0);
  for (const quality of [0.9, 0.8, 0.65, 0.5]) {
    const jpeg = flat.toDataURL("image/jpeg", quality);
    if (dataSize(jpeg) <= IMAGE_MAX) return jpeg;
  }
  return null;
}
