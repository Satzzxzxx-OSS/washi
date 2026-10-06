import * as pdfjs from "pdfjs-dist";
import "pdfjs-dist/web/pdf_viewer.css";
import pdfWorker from "pdfjs-dist/build/pdf.worker.min.mjs?url";

import type { Output } from "../api";
import type { OutlineNode } from "../outline";
import type { Scroll } from "../scroll";
import { clamp, stepZoom } from "../zoom";
import type { RenderContext, View, ZoomDirection } from "./view";

pdfjs.GlobalWorkerOptions.workerSrc = pdfWorker;

const MAX_PAGE_WIDTH = 960;
const GUTTER = 48;
const PRELOAD_MARGIN_PX = 900;

type Slot = {
  wrapper: HTMLElement;
  canvas: HTMLCanvasElement;
  textLayer: HTMLElement;
  page: pdfjs.PDFPageProxy;
  cssViewport: pdfjs.PageViewport;
  canvasViewport: pdfjs.PageViewport;
  painted: boolean;
};

function createSlot(page: pdfjs.PDFPageProxy, cssWidth: number, dpr: number): Slot {
  const natural = page.getViewport({ scale: 1 });
  const scale = cssWidth / natural.width;
  const cssViewport = page.getViewport({ scale });
  const canvasViewport = page.getViewport({ scale: scale * dpr });

  const wrapper = document.createElement("div");
  wrapper.className = "page";
  wrapper.style.width = `${Math.floor(cssViewport.width)}px`;
  wrapper.style.height = `${Math.floor(cssViewport.height)}px`;
  wrapper.style.setProperty("--total-scale-factor", String(scale));
  wrapper.dataset.page = String(page.pageNumber);
  wrapper.dataset.scale = String(scale);

  const canvas = document.createElement("canvas");
  canvas.width = Math.floor(canvasViewport.width);
  canvas.height = Math.floor(canvasViewport.height);
  canvas.style.width = "100%";
  canvas.style.height = "100%";

  const textLayer = document.createElement("div");
  textLayer.className = "textLayer";

  wrapper.append(canvas, textLayer);
  return { wrapper, canvas, textLayer, page, cssViewport, canvasViewport, painted: false };
}

async function paint(slot: Slot) {
  await slot.page.render({
    canvas: slot.canvas,
    viewport: slot.canvasViewport,
  }).promise;
  const layer = new pdfjs.TextLayer({
    textContentSource: slot.page.streamTextContent(),
    container: slot.textLayer,
    viewport: slot.cssViewport,
  });
  await layer.render();
}

type OutlineItem = NonNullable<
  Awaited<ReturnType<pdfjs.PDFDocumentProxy["getOutline"]>>
>[number];

async function pageIndexOf(
  doc: pdfjs.PDFDocumentProxy,
  dest: string | unknown[] | null,
): Promise<number | null> {
  try {
    const explicit = typeof dest === "string" ? await doc.getDestination(dest) : dest;
    if (!Array.isArray(explicit)) return null;
    const ref = explicit[0];
    return typeof ref === "object" && ref !== null
      ? await doc.getPageIndex(ref as Parameters<typeof doc.getPageIndex>[0])
      : Number(ref);
  } catch {
    return null;
  }
}

export class PdfView implements View {
  readonly accepts = "pdf" as const;

  private doc: pdfjs.PDFDocumentProxy | null = null;
  private observer: IntersectionObserver | null = null;
  private zoomLevel = 1;
  private sequence = 0;

  constructor(
    readonly el: HTMLElement,
    private readonly scroll: Scroll,
  ) {}

  async show(output: Output, ctx: RenderContext) {
    if (output.kind !== "pdf") return;
    const next = await pdfjs.getDocument({ data: output.bytes }).promise;
    if (!ctx.isCurrent()) {
      void next.loadingTask.destroy();
      return;
    }
    void this.doc?.loadingTask.destroy();
    this.doc = next;
    await this.layout(ctx);
  }

  relayout(ctx: RenderContext) {
    return this.layout(ctx);
  }

  zoom(direction: ZoomDirection, ctx: RenderContext) {
    this.zoomLevel = stepZoom(this.zoomLevel, direction);
    return this.layout(ctx);
  }

  resetZoom() {
    this.zoomLevel = 1;
  }

  async outline(): Promise<OutlineNode[]> {
    const doc = this.doc;
    if (!doc) return [];
    const items = (await doc.getOutline()) ?? [];
    const nodes: OutlineNode[] = [];
    const walk = async (list: OutlineItem[], level: number) => {
      for (const item of list) {
        const pageIndex = await pageIndexOf(doc, item.dest);
        if (pageIndex !== null) {
          const page = () => this.el.children[pageIndex] as HTMLElement | undefined;
          nodes.push({
            title: item.title,
            level,
            offset: () => (page() ? this.scroll.offsetOf(page()!) : 0),
            go: () => page() && this.scroll.scrollTo(this.scroll.offsetOf(page()!)),
          });
        }
        await walk(item.items, level + 1);
      }
    };
    await walk(items, 1);
    return nodes;
  }

  private paintWhenNear(slots: Slot[], alive: () => boolean) {
    this.observer?.disconnect();
    const bySlot = new Map<Element, Slot>(slots.map((s) => [s.wrapper, s]));
    this.observer = new IntersectionObserver(
      (entries) => {
        for (const entry of entries) {
          const slot = bySlot.get(entry.target);
          if (!entry.isIntersecting || !slot || slot.painted || !alive()) continue;
          slot.painted = true;
          void paint(slot).catch(() => {
            slot.painted = false;
          });
        }
      },
      { rootMargin: `${PRELOAD_MARGIN_PX}px 0px` },
    );
    for (const slot of slots) this.observer.observe(slot.wrapper);
  }

  private async layout(ctx: RenderContext) {
    const doc = this.doc;
    if (!doc) return;
    const mine = ++this.sequence;
    const alive = () => ctx.isCurrent() && mine === this.sequence;

    const cssWidth = clamp(ctx.width - GUTTER, 200, MAX_PAGE_WIDTH) * this.zoomLevel;
    const dpr = window.devicePixelRatio || 1;

    const slots: Slot[] = [];
    for (let n = 1; n <= doc.numPages; n++) {
      const page = await doc.getPage(n);
      if (!alive()) return;
      slots.push(createSlot(page, cssWidth, dpr));
    }
    this.el.replaceChildren(...slots.map((s) => s.wrapper));
    ctx.restoreScroll();
    this.paintWhenNear(slots, alive);
  }
}
