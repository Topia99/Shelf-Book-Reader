import * as pdfjs from "pdfjs-dist";
import type { PDFDocumentProxy } from "pdfjs-dist";
import workerUrl from "pdfjs-dist/build/pdf.worker.min.mjs?url";
import { convertFileSrc } from "@tauri-apps/api/core";

pdfjs.GlobalWorkerOptions.workerSrc = workerUrl;

export { pdfjs };
export type { PDFDocumentProxy };

/** 通过 Tauri asset 协议打开书库内的 PDF（支持 Range，大文件不必整读进内存） */
export function openPdf(filePath: string, password?: string) {
  return pdfjs.getDocument({
    url: convertFileSrc(filePath),
    password,
    cMapUrl: undefined,
  });
}

export function isPasswordError(e: unknown): boolean {
  return (
    typeof e === "object" && e !== null && (e as { name?: string }).name === "PasswordException"
  );
}

/** 渲染第一页为封面 PNG 字节。渲染宽度约 320px。 */
/** 首页封面缩略图：480px 宽 JPEG（体积小，便于云同步与配额；另端下载即用）。 */
export async function renderCoverJpeg(doc: PDFDocumentProxy): Promise<number[]> {
  const page = await doc.getPage(1);
  const base = page.getViewport({ scale: 1 });
  const scale = 480 / base.width;
  const viewport = page.getViewport({ scale });
  const canvas = document.createElement("canvas");
  canvas.width = Math.ceil(viewport.width);
  canvas.height = Math.ceil(viewport.height);
  const ctx = canvas.getContext("2d")!;
  // JPEG 不支持透明，先铺白底避免透明区变黑
  ctx.fillStyle = "#ffffff";
  ctx.fillRect(0, 0, canvas.width, canvas.height);
  await page.render({ canvasContext: ctx, viewport }).promise;
  const blob = await new Promise<Blob>((resolve, reject) =>
    canvas.toBlob((b) => (b ? resolve(b) : reject(new Error("toBlob failed"))), "image/jpeg", 0.82)
  );
  return Array.from(new Uint8Array(await blob.arrayBuffer()));
}
