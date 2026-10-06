import type { Output } from "./api";
import type { OutlineSink } from "./outline";
import type { Scroll } from "./scroll";
import type { RenderContext, View, ZoomDirection } from "./views/view";

export interface Host {
  render(path: string): Promise<Output>;
  renderText(text: string): Promise<Output>;
  opened(path: string): Promise<void>;
  pasted(): Promise<void>;
}

export interface Chrome {
  empty: HTMLElement;
  error: HTMLElement;
}

type Source = { file: string } | { text: string };

export class Viewer {
  private source: Source | null = null;
  private active: View | null = null;
  private token = 0;

  constructor(
    private readonly host: Host,
    private readonly scroll: Scroll,
    private readonly chrome: Chrome,
    private readonly views: View[],
    private readonly outline: OutlineSink = { set: () => {} },
  ) {}

  get currentPath() {
    return this.source && "file" in this.source ? this.source.file : null;
  }

  async load(path: string) {
    this.source = { file: path };
    this.resetZoom();
    try {
      await this.host.opened(path);
    } catch (e) {
      this.showError(String(e));
    }
    await this.reload();
  }

  async showText(text: string) {
    this.source = { text };
    this.resetZoom();
    await this.host.pasted();
    await this.reload();
  }

  async reload() {
    const source = this.source;
    if (!source) return;
    const mine = ++this.token;
    try {
      const output =
        "file" in source
          ? await this.host.render(source.file)
          : await this.host.renderText(source.text);
      if (mine !== this.token) return;
      const view = this.views.find((v) => v.accepts === output.kind);
      if (!view) throw new Error(`表示できない出力です: ${output.kind}`);

      const ratio = view === this.active ? this.scroll.ratio() : 0;
      this.activate(view);
      await view.show(output, this.context(mine, ratio));
      if (mine !== this.token) return;
      this.scroll.restore(ratio);
      this.showError(null);
      this.outline.set((await view.outline?.()) ?? []);
    } catch (e) {
      if (mine === this.token) this.showError(String(e));
    }
  }

  async relayout() {
    const view = this.active;
    if (!view?.relayout) return;
    await view.relayout(this.context(this.token, this.scroll.ratio()));
  }

  async zoom(direction: ZoomDirection) {
    const view = this.active;
    if (!view?.zoom) return;
    await view.zoom(direction, this.context(this.token, this.scroll.ratio()));
  }

  private resetZoom() {
    this.views.forEach((v) => v.resetZoom?.());
  }

  private context(mine: number, ratio: number): RenderContext {
    return {
      width: this.scroll.width,
      isCurrent: () => mine === this.token,
      restoreScroll: () => this.scroll.restore(ratio),
    };
  }

  private activate(view: View) {
    this.chrome.empty.hidden = true;
    for (const v of this.views) v.el.hidden = v !== view;
    this.active = view;
  }

  private showError(message: string | null) {
    this.chrome.error.hidden = message === null;
    this.chrome.error.textContent = message ?? "";
  }
}
