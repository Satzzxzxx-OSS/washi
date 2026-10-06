type FindFn = (
  text: string,
  caseSensitive: boolean,
  backwards: boolean,
  wrap: boolean,
) => boolean;

export class FindBar {
  private readonly input: HTMLInputElement;

  constructor(
    private readonly form: HTMLFormElement,
    private readonly find: FindFn = (text, caseSensitive, backwards, wrap) =>
      (window as unknown as { find: FindFn }).find(text, caseSensitive, backwards, wrap),
  ) {
    this.input = form.querySelector("input")!;
    form.addEventListener("submit", (e) => {
      e.preventDefault();
      this.next(false);
    });
    form.querySelector<HTMLElement>("[data-find=prev]")?.addEventListener("click", () => this.next(true));
    form.querySelector<HTMLElement>("[data-find=next]")?.addEventListener("click", () => this.next(false));
    form.querySelector<HTMLElement>("[data-find=close]")?.addEventListener("click", () => this.close());
    this.input.addEventListener("keydown", (e) => {
      if (e.key === "Escape") this.close();
      else if (e.key === "Enter" && e.shiftKey) {
        e.preventDefault();
        this.next(true);
      }
    });
  }

  get isOpen() {
    return !this.form.hidden;
  }

  open() {
    this.form.hidden = false;
    this.input.focus();
    this.input.select();
  }

  close() {
    this.form.hidden = true;
    getSelection()?.removeAllRanges();
  }

  next(backwards: boolean) {
    const text = this.input.value;
    if (!text) return false;
    const found = this.find(text, false, backwards, true);
    this.form.classList.toggle("not-found", !found);
    if (found) revealSelection();
    this.input.focus();
    return found;
  }
}

function revealSelection() {
  const node = getSelection()?.getRangeAt(0)?.startContainer;
  const target = node instanceof Element ? node : node?.parentElement;
  target?.scrollIntoView({ block: "center", behavior: "smooth" });
}
