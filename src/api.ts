import { invoke } from "@tauri-apps/api/core";

export type Output =
  | { kind: "html"; html: string }
  | { kind: "pdf"; bytes: Uint8Array };

const HTML_TAG = 0;

export function decodeOutput(wire: Uint8Array): Output {
  const body = wire.subarray(1);
  if (wire[0] === HTML_TAG) {
    return { kind: "html", html: new TextDecoder().decode(body) };
  }
  return { kind: "pdf", bytes: body };
}

export async function render(path: string): Promise<Output> {
  const wire = await invoke<ArrayBuffer>("render", { path });
  return decodeOutput(new Uint8Array(wire));
}

export async function renderText(text: string): Promise<Output> {
  const wire = await invoke<ArrayBuffer>("render_text", { text });
  return decodeOutput(new Uint8Array(wire));
}

export type Severity = "error" | "warning";

/** 編集中の本文に対する診断。行と列は 1 始まりで、列は Unicode のコードポイントの数 */
export interface Diagnostic {
  /** `null` は、描画した本文そのもの。ほかのファイルなら、プロジェクトの根からの相対パス */
  file: string | null;
  line: number;
  column: number;
  endLine: number;
  endColumn: number;
  severity: Severity;
  message: string;
  hints: string[];
}

/** 保存前の本文の描画結果。失敗しても診断はある（画面は、直前の良いプレビューを残して、波線だけ更新する） */
export type BufferResult =
  | { ok: true; output: Output; diagnostics: Diagnostic[] }
  | { ok: false; message: string; diagnostics: Diagnostic[] };

const PDF_TAG = 1;
const FAILED_TAG = 2;
const HEADER_BYTES = 5;

interface WireDiagnostic {
  file: string | null;
  line: number;
  column: number;
  end_line: number;
  end_column: number;
  severity: Severity;
  message: string;
  hints: string[];
}

/** `[tag: u8][診断 JSON の長さ: u32 ビッグエンディアン][診断 JSON][本体]`（Rust の `Rendered::into_wire`） */
export function decodeBuffer(wire: Uint8Array): BufferResult {
  const length = new DataView(wire.buffer, wire.byteOffset, wire.byteLength).getUint32(1, false);
  const json = new TextDecoder().decode(wire.subarray(HEADER_BYTES, HEADER_BYTES + length));
  const diagnostics = (JSON.parse(json) as WireDiagnostic[]).map(
    (d): Diagnostic => ({
      file: d.file,
      line: d.line,
      column: d.column,
      endLine: d.end_line,
      endColumn: d.end_column,
      severity: d.severity,
      message: d.message,
      hints: d.hints,
    }),
  );
  const body = wire.subarray(HEADER_BYTES + length);
  if (wire[0] === FAILED_TAG) {
    return { ok: false, message: new TextDecoder().decode(body), diagnostics };
  }
  const output: Output =
    wire[0] === PDF_TAG ? { kind: "pdf", bytes: body } : { kind: "html", html: new TextDecoder().decode(body) };
  return { ok: true, output, diagnostics };
}

export async function renderBuffer(path: string, text: string): Promise<BufferResult> {
  const wire = await invoke<ArrayBuffer>("render_buffer", { path, text });
  return decodeBuffer(new Uint8Array(wire));
}

export interface DiskText {
  text: string;
  hash: string;
}

export type SaveResult = { status: "saved"; hash: string } | { status: "conflict"; disk_hash: string };

export const readText = (path: string) => invoke<DiskText>("read_text", { path });

/** `baseHash` は、読み込んだときのハッシュ。ディスクが変わっていれば `conflict` が返る。`force` のときだけ上書きする */
export const writeFile = (path: string, text: string, baseHash: string | null, force = false) =>
  invoke<SaveResult>("write_file", { path, text, baseHash, force });

export interface CompletionItem {
  label: string;
  apply: string | null;
  detail: string | null;
  kind: string;
}

export interface Completions {
  /** 置き換える範囲の始まり（UTF-16、エディタの位置） */
  offset: number;
  items: CompletionItem[];
}

export const autocomplete = (path: string, text: string, offset: number, explicit: boolean) =>
  invoke<Completions>("autocomplete", { path, text, offset, explicit });

export interface PreviewPosition {
  page: number;
  x: number;
  y: number;
}

/** ソースの行・列（1 始まり）に対応する、プレビュー上の位置（PDF のポイント）。前方検索 */
export const forwardLocate = (path: string, line: number, column: number) =>
  invoke<PreviewPosition | null>("forward_locate", { path, line, column });

export interface LocatedSource {
  file: string;
  line: number;
  column: number;
}

/** プレビューの位置に対応するソースの位置。外部のエディタは起動しない */
export const locateSource = (path: string, page: number, x: number, y: number) =>
  invoke<LocatedSource | null>("locate_source", { path, page, x, y });

/** 未保存の変更があるかを Rust 側にも伝える（⌘Q の確認に使う） */
export const setDirty = (dirty: boolean) => invoke<void>("set_dirty", { dirty });

export const supportedExtensions =() =>
  invoke<string[]>("supported_extensions");

export const initialFile = () => invoke<string | null>("initial_file");

export const jumpToSource = (path: string, page: number, x: number, y: number) =>
  invoke<string | null>("jump_to_source", { path, page, x, y });

export const print = () => invoke<void>("print");

export const watch =(path: string) => invoke<void>("watch", { path });
