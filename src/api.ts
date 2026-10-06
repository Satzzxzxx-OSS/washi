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

export const supportedExtensions =() =>
  invoke<string[]>("supported_extensions");

export const initialFile = () => invoke<string | null>("initial_file");

export const jumpToSource = (path: string, page: number, x: number, y: number) =>
  invoke<string | null>("jump_to_source", { path, page, x, y });

export const print = () => invoke<void>("print");

export const watch =(path: string) => invoke<void>("watch", { path });
