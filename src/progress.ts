/** 読んだ割合（0〜1）。スクロールできなければ `null` */
export function progressOf(scrollTop: number, scrollHeight: number, clientHeight: number): number | null {
  const max = scrollHeight - clientHeight;
  if (!(max > 1)) return null;
  return Math.min(1, Math.max(0, scrollTop / max));
}

/** 画面の上端に出す、読み進んだ割合の細い線 */
export class ReadingProgress {
  private frame = 0;

  constructor(
    private readonly bar: HTMLElement,
    private readonly scroller: HTMLElement,
    watch: HTMLElement[] = [],
  ) {
    scroller.addEventListener("scroll", () => this.schedule(), { passive: true });
    // 描画で文書の高さが変わったときや、ウィンドウの大きさが変わったとき
    const observer = new ResizeObserver(() => this.schedule());
    observer.observe(scroller);
    for (const el of watch) observer.observe(el);
    this.schedule();
  }

  private schedule() {
    if (this.frame) return;
    this.frame = requestAnimationFrame(() => {
      this.frame = 0;
      this.update();
    });
  }

  update() {
    const { scrollTop, scrollHeight, clientHeight } = this.scroller;
    const progress = progressOf(scrollTop, scrollHeight, clientHeight);
    this.bar.hidden = progress === null;
    if (progress !== null) this.bar.style.setProperty("--progress", String(progress));
  }
}
