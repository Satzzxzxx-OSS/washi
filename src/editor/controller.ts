import { autocomplete, forwardLocate, locateSource, readText, writeFile, type Diagnostic } from "../api";
import { basename } from "../paths";
import type { Prefs } from "../prefs";
import type { Scroll } from "../scroll";
import type { Viewer } from "../viewer";
import type { EditorHandle } from "./editor";
import { DIRTY_MARK, kindOf, type Kind } from "./kinds";
import { EditSession, type Banner, type SessionHost } from "./session";
import { applyEditorWidth, attachResizer } from "./split";
import { elementForLine, sourceAt } from "./sync";

/** カーソルが止まってから、プレビューをその行に合わせるまでの時間 */
const REVEAL_DELAY_MS = 150;
/** 自分の保存で起きた変更の通知を、外からの変更と区別する時間 */
const OWN_WRITE_WINDOW_MS = 800;
const FLASH_MS = 1200;

export interface Elements {
  pane: HTMLElement;
  editor: HTMLElement;
  banner: HTMLElement;
  status: HTMLElement;
  resizer: HTMLElement;
  confirm: HTMLDialogElement;
  scroller: HTMLElement;
  markdown: HTMLElement;
  pdf: HTMLElement;
}

export interface Deps {
  viewer: Viewer;
  scroll: Scroll;
  elements: Elements;
  prefs(): Prefs;
  update(patch: Partial<Prefs>): void;
  toast(message: string, ms?: number): void;
  setTitle(title: string): Promise<void> | void;
  /** ウィンドウに未保存の変更があるか（⌘Q の確認に使う） */
  setDirty(dirty: boolean): Promise<void> | void;
}

type Choice = "save" | "discard" | "cancel";

function button(label: string, onClick: () => void) {
  const b = document.createElement("button");
  b.type = "button";
  b.textContent = label;
  b.addEventListener("click", onClick);
  return b;
}

/** 編集（分割表示）の全体。エディタ・保存・ディスクの変更・プレビューとの位置合わせを束ねる */
export class EditingController {
  private session: EditSession | null = null;
  private handle: EditorHandle | null = null;
  private kind: Kind | null = null;
  private lastWrite = 0;
  private revealTimer: number | undefined;
  private lastRevealLine = 0;
  private failure: string | null = null;
  private entering = false;

  constructor(private readonly deps: Deps) {
    const { elements: el } = deps;
    attachResizer(
      el.resizer,
      el.pane,
      () => deps.prefs().editorWidth,
      (percent) => {
        deps.update({ editorWidth: percent });
        applyEditorWidth(el.pane, percent);
      },
    );
    // プレビュー（Markdown）の ⌘クリックで、ソースの位置へ
    el.markdown.addEventListener(
      "click",
      (e) => {
        if (!e.metaKey || !this.session) return;
        const at = sourceAt(e.target as Element);
        if (!at) return;
        e.preventDefault();
        e.stopPropagation();
        this.handle?.goTo(at.line, at.column);
      },
      true,
    );
  }

  get active() {
    return this.session !== null;
  }

  get dirty() {
    return this.session?.dirty ?? false;
  }

  /** ⌘E: 読む ⇄ 分割 */
  async toggle() {
    if (this.entering) return;
    if (this.session) {
      if (!(await this.release())) return;
      await this.deps.viewer.leaveBuffer();
      return;
    }
    await this.enter();
  }

  private async enter() {
    const { viewer, elements: el } = this.deps;
    const path = viewer.currentPath;
    const kind = path ? kindOf(path) : null;
    if (!path || !kind) {
      this.deps.toast(path ? "この種類のファイルは編集できません" : "ファイルを開いてから編集してください");
      return;
    }
    this.entering = true;
    try {
      const [{ createEditor }, { languageFor }, disk] = await Promise.all([
        import("./editor"),
        import("./languages"),
        readText(path),
      ]);
      const [language, extra] = await Promise.all([languageFor(kind), this.extensionsFor(kind, path)]);
      const session = new EditSession(this.sessionHost(path), path, disk, {
        kind,
        autosave: () => this.deps.prefs().autosave,
      });
      this.session = session;
      this.kind = kind;
      this.failure = null;
      applyEditorWidth(el.pane, this.deps.prefs().editorWidth);
      el.pane.hidden = false;
      el.resizer.hidden = false;
      this.handle = createEditor(el.editor, {
        doc: disk.text,
        language,
        extra,
        onChange: (text) => session.edit(text),
        onCursor: (position) => {
          this.showStatus(position.line, position.column);
          this.revealSoon();
        },
      });
      viewer.setBufferHooks({
        diagnostics: (list) => this.handle?.setDiagnostics(this.ownDiagnostics(list)),
        failed: (message) => {
          this.failure = message;
          this.refreshStatus();
        },
      });
      this.refreshStatus();
      session.start();
      this.handle.focus();
    } catch (e) {
      this.teardown();
      this.deps.toast(`編集を始められませんでした: ${String(e)}`, 5000);
    } finally {
      this.entering = false;
    }
  }

  private async extensionsFor(kind: Kind, path: string) {
    if (kind !== "typst") return [];
    const { typstCompletion } = await import("./complete");
    return [typstCompletion((text, offset, explicit) => autocomplete(path, text, offset, explicit))];
  }

  private sessionHost(path: string): SessionHost {
    const { viewer } = this.deps;
    return {
      readText,
      writeFile: async (p, text, base, force) => {
        try {
          return await writeFile(p, text, base, force);
        } finally {
          this.lastWrite = performance.now();
        }
      },
      render: (p, text) => viewer.renderBuffer(p, text),
      setText: (text) => this.handle?.setText(text),
      dirtyChanged: (dirty) => {
        void this.deps.setTitle(`${dirty ? DIRTY_MARK : ""}${basename(path)} — Washi`);
        void this.deps.setDirty(dirty);
      },
      banner: (banner) => this.showBanner(banner),
      notify: (message) => this.deps.toast(message, 5000),
    };
  }

  /** 開いているファイル自身の診断だけを、エディタに出す */
  private ownDiagnostics(list: readonly Diagnostic[]) {
    return list.filter((d) => d.file === null);
  }

  /** 編集をやめる。未保存なら確認する。`false` はキャンセル（そのまま編集を続ける） */
  async release(): Promise<boolean> {
    const session = this.session;
    if (!session) return true;
    if (!(await this.confirmDiscardOrSave())) return false;
    this.teardown();
    return true;
  }

  /** 未保存の変更があれば、保存・破棄・キャンセルを選ばせる。`true` は、閉じてよい */
  async confirmDiscardOrSave(): Promise<boolean> {
    const session = this.session;
    if (!session?.dirty) return true;
    const choice = await this.ask(`「${basename(session.path)}」の変更を保存しますか？`);
    if (choice === "cancel") return false;
    if (choice === "discard") return true;
    const outcome = await session.save();
    return outcome === "saved" || outcome === "clean";
  }

  private ask(message: string): Promise<Choice> {
    const dialog = this.deps.elements.confirm;
    dialog.querySelector("p")!.textContent = message;
    return new Promise((resolve) => {
      dialog.addEventListener(
        "close",
        () => resolve(dialog.returnValue === "save" || dialog.returnValue === "discard" ? dialog.returnValue : "cancel"),
        { once: true },
      );
      dialog.returnValue = "";
      dialog.showModal();
    });
  }

  private teardown() {
    const { elements: el, viewer } = this.deps;
    clearTimeout(this.revealTimer);
    this.session?.dispose();
    this.handle?.destroy();
    this.session = null;
    this.handle = null;
    this.kind = null;
    this.failure = null;
    viewer.setBufferHooks(null);
    el.pane.hidden = true;
    el.resizer.hidden = true;
    el.editor.replaceChildren();
    el.banner.hidden = true;
    void this.deps.setDirty(false);
  }

  async save() {
    const session = this.session;
    if (!session) return;
    const outcome = await session.save();
    if (outcome === "saved") this.deps.toast("保存しました", 1200);
  }

  /** ディスクのファイルが変わった通知。編集中なら扱って `true` を返す（読むモードの再読み込みは不要） */
  async diskChanged(): Promise<boolean> {
    const session = this.session;
    if (!session) return false;
    if (performance.now() - this.lastWrite < OWN_WRITE_WINDOW_MS) return true;
    const change = await session.diskChanged();
    // 本文が同じなら、読み込み先（章・画像など）が変わったのかもしれない
    if (change === "ignore") session.refreshPreview();
    return true;
  }

  find() {
    if (!this.handle?.hasFocus()) return false;
    this.handle.openSearch();
    return true;
  }

  undo() {
    if (this.handle?.hasFocus()) this.handle.undo();
    else document.execCommand("undo");
  }

  redo() {
    if (this.handle?.hasFocus()) this.handle.redo();
    else document.execCommand("redo");
  }

  /** エディタにフォーカスがあれば、貼り付けを引き受ける */
  paste(text: string) {
    if (!this.handle?.hasFocus()) return false;
    this.handle.replaceSelection(text);
    return true;
  }

  /** プレビュー（PDF）の ⌘クリック。開いているファイルのソースなら、エディタのカーソルを動かして `true` */
  async revealSource(page: number, x: number, y: number): Promise<boolean> {
    const path = this.session?.path;
    if (!path || !this.handle) return false;
    try {
      const found = await locateSource(path, page, x, y);
      if (!found || found.file !== path) return false;
      this.handle.goTo(found.line, found.column);
      return true;
    } catch {
      return false;
    }
  }

  toggleAutosave() {
    const autosave = !this.deps.prefs().autosave;
    this.deps.update({ autosave });
    this.deps.toast(`自動保存: ${autosave ? "オン" : "オフ"}`);
  }

  toggleSyncCursor() {
    const syncCursor = !this.deps.prefs().syncCursor;
    this.deps.update({ syncCursor });
    this.deps.toast(`カーソルの行をプレビューに表示: ${syncCursor ? "オン" : "オフ"}`);
  }

  // ---- 通知とステータス ----

  private showBanner(banner: Banner) {
    const el = this.deps.elements.banner;
    el.replaceChildren();
    if (!banner) {
      el.hidden = true;
      return;
    }
    el.hidden = false;
    el.dataset.kind = banner.kind;
    const text = document.createElement("span");
    el.append(text);
    if (banner.kind === "conflict") {
      text.textContent = "ディスク上のファイルが、別の場所で変更されました。";
      el.append(
        button("自分の版を保つ", () => this.session?.keepMine()),
        button("ディスクの版を読み込む", () => void this.session?.loadDisk()),
      );
    } else {
      text.textContent = `保存できませんでした: ${banner.message}`;
      el.append(button("閉じる", () => this.showBanner(null)));
    }
  }

  private lastPosition = { line: 1, column: 1 };

  private showStatus(line: number, column: number) {
    this.lastPosition = { line, column };
    this.refreshStatus();
  }

  private refreshStatus() {
    const { line, column } = this.lastPosition;
    const status = this.deps.elements.status;
    status.replaceChildren();
    const pos = document.createElement("span");
    pos.textContent = `${line}:${column}`;
    status.append(pos);
    if (this.failure) {
      const fail = document.createElement("span");
      fail.className = "failure";
      fail.textContent = `プレビューを更新できません: ${this.failure.split("\n")[0]}`;
      fail.title = this.failure;
      status.append(fail);
    }
  }

  // ---- ソース → プレビューの位置合わせ ----

  private revealSoon() {
    if (!this.deps.prefs().syncCursor) return;
    clearTimeout(this.revealTimer);
    this.revealTimer = window.setTimeout(() => void this.reveal(), REVEAL_DELAY_MS);
  }

  private async reveal() {
    const handle = this.handle;
    const path = this.session?.path;
    if (!handle || !path || !this.kind) return;
    const { line, column } = handle.cursor();
    // 同じ行のままなら、プレビューを動かさない（読んでいる位置を邪魔しない）
    if (line === this.lastRevealLine) return;
    this.lastRevealLine = line;
    const { scroll, elements: el } = this.deps;
    if (this.kind === "markdown") {
      const target = elementForLine(el.markdown, line);
      if (!target) return;
      scroll.scrollTo(scroll.offsetOf(target) - el.scroller.clientHeight / 3);
      target.classList.add("sync-flash");
      window.setTimeout(() => target.classList.remove("sync-flash"), FLASH_MS);
      return;
    }
    if (this.kind === "mermaid") return;
    try {
      const at = await forwardLocate(path, line, column);
      if (!at || !this.session) return;
      const wrapper = el.pdf.querySelector<HTMLElement>(`.page[data-page="${at.page}"]`);
      const scale = Number(wrapper?.dataset.scale);
      if (!wrapper || !scale) return;
      scroll.scrollTo(scroll.offsetOf(wrapper) + at.y * scale - el.scroller.clientHeight / 3);
      const marker = document.createElement("div");
      marker.className = "sync-marker";
      marker.style.left = `${at.x * scale}px`;
      marker.style.top = `${at.y * scale}px`;
      wrapper.append(marker);
      window.setTimeout(() => marker.remove(), FLASH_MS);
    } catch {
      // 位置が分からないのは、よくあること。何もしない
    }
  }
}
