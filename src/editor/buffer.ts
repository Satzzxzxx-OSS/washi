/** エディタで編集中の本文と、ディスクとの関係。DOM に依存しない純粋な状態。 */

export interface Disk {
  text: string;
  hash: string;
}

export class Buffer {
  text: string;
  /** 最後に保存した、または読み込んだ内容 */
  private saved: string;
  /** 最後に保存した、または読み込んだときの、ディスクの内容のハッシュ。保存のときの衝突の検知に使う */
  baseHash: string;

  constructor(disk: Disk) {
    this.text = disk.text;
    this.saved = disk.text;
    this.baseHash = disk.hash;
  }

  get dirty() {
    return this.text !== this.saved;
  }

  edit(text: string) {
    this.text = text;
  }

  /** `text` を書き込めた。それが、新しい「保存した内容」 */
  markSaved(text: string, hash: string) {
    this.saved = text;
    this.baseHash = hash;
  }

  /** ディスクの内容を取り込む（自分の変更は捨てる） */
  reload(disk: Disk) {
    this.text = disk.text;
    this.saved = disk.text;
    this.baseHash = disk.hash;
  }

  /** 「自分の版を保つ」: ディスクの内容は取り込まず、次の保存で意図して上書きする */
  rebase(diskHash: string) {
    this.baseHash = diskHash;
  }
}

export type DiskChange = "ignore" | "reload" | "conflict";

/**
 * ディスクのファイルが変わったという通知が来たときの扱い。
 * - ハッシュが同じ: 自分の保存か、変わっていない。無視する
 * - 自分は未変更: 静かに読み直す
 * - 自分は未保存で、内容が違う: 衝突。黙ってどちらかを捨てず、選ばせる
 */
export function onDiskChange(buffer: Buffer, disk: Disk): DiskChange {
  if (disk.hash === buffer.baseHash) return "ignore";
  if (!buffer.dirty) return "reload";
  if (disk.text === buffer.text) return "reload";
  return "conflict";
}
