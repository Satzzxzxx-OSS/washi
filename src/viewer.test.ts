import { describe, expect, it } from "vitest";
import type { Output } from "./api";
import type { Scroll } from "./scroll";
import { Viewer, type Host } from "./viewer";
import type { RenderContext, View, ZoomDirection } from "./views/view";

const el = () => ({ hidden: true, textContent: "" }) as unknown as HTMLElement;

class FakeScroll {
  position = 0;
  width = 800;
  ratio() {
    return this.position;
  }
  restore(r: number) {
    this.position = r;
  }
}

class FakeView implements View {
  shown: string[] = [];
  zooms: ZoomDirection[] = [];
  el = el();
  constructor(readonly accepts: Output["kind"]) {}
  async show(output: Output, ctx: RenderContext) {
    this.shown.push(output.kind === "html" ? output.html : "pdf");
    ctx.restoreScroll();
  }
  async zoom(direction: ZoomDirection) {
    this.zooms.push(direction);
  }
}

type Deferred = { resolve: (o: Output) => void };

function setup() {
  const pending = new Map<string, Deferred>();
  const titles: string[] = [];
  const host: Host = {
    render: (path) =>
      new Promise((resolve) => pending.set(path, { resolve })),
    renderText: async (text) => ({ kind: "html", html: `text:${text}` }),
    opened: async (path) => void titles.push(path),
    pasted: async () => void titles.push("pasted"),
  };
  const scroll = new FakeScroll();
  const chrome = { empty: el(), error: el() };
  const html = new FakeView("html");
  const pdf = new FakeView("pdf");
  const viewer = new Viewer(host, scroll as unknown as Scroll, chrome, [html, pdf]);
  return { viewer, scroll, chrome, html, pdf, pending, titles };
}

const flush = () => new Promise((r) => setTimeout(r, 0));

describe("Viewer", () => {
  it("shows the view that accepts the output and hides the others", async () => {
    const t = setup();
    const done = t.viewer.load("/a.md");
    await flush();
    t.pending.get("/a.md")!.resolve({ kind: "html", html: "A" });
    await done;
    expect(t.html.shown).toEqual(["A"]);
    expect(t.html.el.hidden).toBe(false);
    expect(t.pdf.el.hidden).toBe(true);
    expect(t.chrome.empty.hidden).toBe(true);
    expect(t.chrome.error.hidden).toBe(true);
    expect(t.titles).toEqual(["/a.md"]);
  });

  it("discards a stale render when a newer one finishes first", async () => {
    const t = setup();
    const first = t.viewer.load("/slow.md");
    await flush();
    const second = t.viewer.load("/fast.md");
    await flush();
    t.pending.get("/fast.md")!.resolve({ kind: "html", html: "fast" });
    await second;
    t.pending.get("/slow.md")!.resolve({ kind: "html", html: "slow" });
    await first;
    expect(t.html.shown).toEqual(["fast"]);
  });

  it("keeps the scroll position when the same view re-renders", async () => {
    const t = setup();
    const load = t.viewer.load("/a.md");
    await flush();
    t.pending.get("/a.md")!.resolve({ kind: "html", html: "1" });
    await load;
    t.scroll.position = 0.6;

    const again = t.viewer.reload();
    await flush();
    t.pending.get("/a.md")!.resolve({ kind: "html", html: "2" });
    await again;
    expect(t.scroll.position).toBe(0.6);
  });

  it("resets the scroll position when the view changes", async () => {
    const t = setup();
    const load = t.viewer.load("/a.md");
    await flush();
    t.pending.get("/a.md")!.resolve({ kind: "html", html: "1" });
    await load;
    t.scroll.position = 0.6;

    const next = t.viewer.load("/b.typ");
    await flush();
    t.pending.get("/b.typ")!.resolve({ kind: "pdf", bytes: new Uint8Array() });
    await next;
    expect(t.pdf.shown).toEqual(["pdf"]);
    expect(t.scroll.position).toBe(0);
  });

  it("shows errors and clears them on the next success", async () => {
    const failing: Host = {
      render: async () => {
        throw new Error("boom");
      },
      renderText: async () => ({ kind: "html", html: "ok" }),
      opened: async () => {},
      pasted: async () => {},
    };
    const chrome = { empty: el(), error: el() };
    const viewer = new Viewer(failing, new FakeScroll() as unknown as Scroll, chrome, [new FakeView("html")]);
    await viewer.load("/x.md");
    expect(chrome.error.hidden).toBe(false);
    expect(chrome.error.textContent).toContain("boom");

    await viewer.showText("fine");
    expect(chrome.error.hidden).toBe(true);
  });

  it("renders pasted text and has no current path", async () => {
    const t = setup();
    await t.viewer.showText("hello");
    expect(t.html.shown).toEqual(["text:hello"]);
    expect(t.viewer.currentPath).toBeNull();
    expect(t.titles).toEqual(["pasted"]);
  });

  it("forwards zoom to the active view only", async () => {
    const t = setup();
    await t.viewer.zoom("in");
    expect(t.html.zooms).toEqual([]);
    await t.viewer.showText("x");
    await t.viewer.zoom("in");
    expect(t.html.zooms).toEqual(["in"]);
    expect(t.pdf.zooms).toEqual([]);
  });
});
