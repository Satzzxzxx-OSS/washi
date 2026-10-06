import type { SaveResult } from "../api";
import { Buffer, onDiskChange, type Disk, type DiskChange } from "./buffer";
import { renderPolicy, type Kind } from "./kinds";
import { Scheduler, type Timers } from "./scheduler";

/** エディタの上に出す通知 */
export type Banner =
  | null
  | { kind: "conflict" }
  | { kind: "error"; message: string };

/** 自動保存: 入力が止まって `AUTOSAVE_IDLE_MS`、ただし最初の変更から `AUTOSAVE_MAX_MS` を超えない */
export const AUTOSAVE_IDLE_MS = 1000;
export const AUTOSAVE_MAX_MS = 5000;

/** セッションが外の世界とやりとりするための口。テストでは偽物を渡す。 */
export interface SessionHost {
  readText(path: string): Promise<Disk>;
  writeFile(path: string, text: string, baseHash: string | null, force: boolean): Promise<SaveResult>;
  /** プレビューを、この本文で描き直す */
  render(path: string, text: string): Promise<unknown>;
  /** エディタの本文を置き換える（ディスクの内容を取り込んだとき） */
  setText(text: string): void;
  dirtyChanged(dirty: boolean): void;
  banner(banner: Banner): void;
  notify(message: string): void;
}

export interface SessionOptions {
  kind: Kind;
  autosave(): boolean;
  timers?: Timers;
}

export type SaveOutcome = "saved" | "clean" | "conflict" | "failed";

const realTimers: Timers = {
  now: () => performance.now(),
  set: (fn, ms) => window.setTimeout(fn, ms),
  clear: (handle) => window.clearTimeout(handle as number),
};

/**
 * 1 つのファイルを編集している間の、本文・プレビュー・保存・ディスクの変更の扱い。
 * DOM に依存しない。エディタとビューアは、`SessionHost` を通して繋ぐ。
 */
export class EditSession {
  readonly buffer: Buffer;
  private readonly scheduler: Scheduler;
  private readonly timers: Timers;
  private idleTimer: unknown;
  private maxTimer: unknown;
  private lastDirty = false;
  private conflict: Disk | null = null;
  private saving: Promise<SaveOutcome> | null = null;

  constructor(
    private readonly host: SessionHost,
    readonly path: string,
    disk: Disk,
    private readonly options: SessionOptions,
  ) {
    this.timers = options.timers ?? realTimers;
    this.buffer = new Buffer(disk);
    const policy = renderPolicy(options.kind);
    this.scheduler = new Scheduler({
      ...policy,
      timers: this.timers,
      run: () => this.host.render(this.path, this.buffer.text),
    });
  }

  get dirty() {
    return this.buffer.dirty;
  }

  get text() {
    return this.buffer.text;
  }

  /** 編集を始める。いまの本文でプレビューを描く */
  start() {
    this.scheduler.flush();
  }

  /** エディタで本文が変わった */
  edit(text: string) {
    if (text === this.buffer.text) return;
    this.buffer.edit(text);
    this.scheduler.trigger();
    this.syncDirty();
    if (this.options.autosave()) this.armAutosave();
  }

  async save(): Promise<SaveOutcome> {
    // 保存の最中にもう一度保存が来たら、前の保存の終わりを待ってから
    if (this.saving) await this.saving.catch(() => {});
    this.saving = this.doSave();
    try {
      return await this.saving;
    } finally {
      this.saving = null;
    }
  }

  private async doSave(): Promise<SaveOutcome> {
    this.disarmAutosave();
    if (!this.buffer.dirty) return "clean";
    const text = this.buffer.text;
    let result: SaveResult;
    try {
      result = await this.host.writeFile(this.path, text, this.buffer.baseHash, false);
    } catch (e) {
      const message = String(e);
      this.host.banner({ kind: "error", message });
      this.host.notify(message);
      return "failed";
    }
    if (result.status === "conflict") {
      this.conflict = await this.readDisk();
      this.host.banner({ kind: "conflict" });
      return "conflict";
    }
    this.buffer.markSaved(text, result.hash);
    this.host.banner(null);
    this.syncDirty();
    // LaTeX は、保存したときは待たずに描く
    this.scheduler.flush();
    return "saved";
  }

  /**
   * ディスクのファイルが変わったという通知が来た。何をしたかを返す（`unreadable` は、読めなかった）。
   * `ignore` のときは本文が変わっていないので、呼び出し側は、読み込み先のファイルの変更を反映するために、プレビューを描き直せる
   */
  async diskChanged(): Promise<DiskChange | "unreadable"> {
    const disk = await this.readDisk();
    if (!disk) return "unreadable";
    const change = onDiskChange(this.buffer, disk);
    if (change === "reload") this.adopt(disk);
    if (change === "conflict") {
      this.conflict = disk;
      this.host.banner({ kind: "conflict" });
    }
    return change;
  }

  /** 読み込み先のファイル（章、画像など）が変わったので、いまの本文でプレビューを描き直す */
  refreshPreview() {
    this.scheduler.flush();
  }

  /** 衝突の通知で「自分の版を保つ」: 次の保存で、意図してディスクの内容を上書きする */
  keepMine() {
    if (!this.conflict) return;
    this.buffer.rebase(this.conflict.hash);
    this.conflict = null;
    this.host.banner(null);
  }

  /** 衝突の通知で「ディスクの版を読み込む」: 自分の変更は捨てる */
  async loadDisk() {
    const disk = this.conflict ?? (await this.readDisk());
    this.conflict = null;
    this.host.banner(null);
    if (disk) this.adopt(disk);
  }

  dispose() {
    this.scheduler.cancel();
    this.disarmAutosave();
  }

  private adopt(disk: Disk) {
    this.buffer.reload(disk);
    this.host.setText(disk.text);
    this.syncDirty();
    this.host.banner(null);
    this.scheduler.flush();
  }

  private async readDisk(): Promise<Disk | null> {
    try {
      return await this.host.readText(this.path);
    } catch {
      return null;
    }
  }

  private syncDirty() {
    if (this.lastDirty === this.buffer.dirty) return;
    this.lastDirty = this.buffer.dirty;
    this.host.dirtyChanged(this.lastDirty);
  }

  private armAutosave() {
    if (this.idleTimer !== undefined) this.timers.clear(this.idleTimer);
    this.idleTimer = this.timers.set(() => void this.autosave(), AUTOSAVE_IDLE_MS);
    this.maxTimer ??= this.timers.set(() => void this.autosave(), AUTOSAVE_MAX_MS);
  }

  private disarmAutosave() {
    if (this.idleTimer !== undefined) this.timers.clear(this.idleTimer);
    if (this.maxTimer !== undefined) this.timers.clear(this.maxTimer);
    this.idleTimer = undefined;
    this.maxTimer = undefined;
  }

  private async autosave() {
    this.disarmAutosave();
    // 衝突しているときは、勝手に上書きしない
    if (this.conflict) return;
    await this.save();
  }
}
