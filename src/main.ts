import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { readText } from "@tauri-apps/plugin-clipboard-manager";
import { open } from "@tauri-apps/plugin-dialog";
import { openUrl } from "@tauri-apps/plugin-opener";

import { menuActions } from "./actions";
import {
  initialFile,
  jumpToSource,
  print,
  render,
  renderBuffer,
  renderText,
  setDirty,
  supportedExtensions,
  watch,
} from "./api";
import { interpretPaste } from "./clipboard";
import { EditingController } from "./editor/controller";
import { FindBar } from "./find";
import { hasJumpableSource, pageClick } from "./jump";
import { OutlinePanel } from "./outline";
import { basename, extensionOf, resolveLink } from "./paths";
import { loadPrefs, savePrefs, type Prefs } from "./prefs";
import { Scroll } from "./scroll";
import { applyPrefs } from "./theme";
import { createToast } from "./toast";
import { Viewer, type Host } from "./viewer";
import { MarkdownView } from "./views/markdown";
import { PdfView } from "./views/pdf";

const CHANGED_EVENT = "washi://changed";
const OPEN_EVENT = "washi://open";
const MENU_EVENT = "washi://menu";
const QUIT_EVENT = "washi://quit-requested";
const RELOAD_DELAY_MS = 150;
const WHEEL_ZOOM_INTERVAL_MS = 90;

const byId = (id: string) => document.getElementById(id)!;

function debounce(fn: () => void, ms: number) {
  let timer: number | undefined;
  return () => {
    clearTimeout(timer);
    timer = window.setTimeout(fn, ms);
  };
}

const host: Host = {
  render,
  renderBuffer,
  renderText,
  async opened(path) {
    await getCurrentWindow().setTitle(`${basename(path)} — Washi`);
    await watch(path);
  },
  async pasted() {
    await getCurrentWindow().setTitle("Pasted text — Washi");
  },
};

async function main() {
  const extensions = await supportedExtensions();
  const isSupported = (path: string) => extensions.includes(extensionOf(path));

  const root = document.documentElement;
  let prefs = loadPrefs();
  applyPrefs(root, prefs);

  const scroll = new Scroll(byId("scroller"));
  const outline = new OutlinePanel(byId("outline"), byId("outline").querySelector("nav")!, scroll);
  outline.show(prefs.outline);

  const viewer = new Viewer(
    host,
    scroll,
    { empty: byId("empty"), error: byId("error") },
    [new MarkdownView(byId("markdown"), scroll), new PdfView(byId("pdf"), scroll)],
    outline,
  );
  const finder = new FindBar(byId("find") as HTMLFormElement);
  const toast = createToast(byId("toast"));

  const editing = new EditingController({
    viewer,
    scroll,
    elements: {
      pane: byId("editor-pane"),
      editor: byId("editor"),
      banner: byId("banner"),
      status: byId("editor-status"),
      resizer: byId("resizer"),
      confirm: byId("confirm") as HTMLDialogElement,
      scroller: byId("scroller"),
      markdown: byId("markdown"),
      pdf: byId("pdf"),
    },
    prefs: () => prefs,
    update: (patch) => update(patch),
    toast,
    setTitle: (title) => getCurrentWindow().setTitle(title),
    setDirty,
  });

  /** 別のファイルを開く。編集中なら、先に保存・破棄を確認する（キャンセルなら開かない） */
  const open_ = async (path: string) => {
    if (await editing.release()) await viewer.load(path);
  };

  const reloadSoon = debounce(() => void viewer.reload(), RELOAD_DELAY_MS);
  const relayoutSoon = debounce(() => void viewer.relayout(), RELOAD_DELAY_MS);

  const update = (patch: Partial<Prefs>) => {
    prefs = { ...prefs, ...patch };
    savePrefs(prefs);
    applyPrefs(root, prefs);
  };

  const pick = async () => {
    const selected = await open({
      multiple: false,
      filters: [{ name: "Documents", extensions }],
    });
    if (typeof selected === "string") await open_(selected);
  };

  const paste = async () => {
    const text = await readText().catch(() => "");
    const field = document.activeElement;
    if (editing.paste(text)) return;
    if (field instanceof HTMLInputElement) {
      const end = field.value.length;
      field.setRangeText(text, field.selectionStart ?? end, field.selectionEnd ?? end, "end");
      return;
    }
    const pasted = interpretPaste(text, isSupported);
    if (pasted && !(await editing.release())) return;
    if (pasted?.kind === "file") await viewer.load(pasted.path);
    else if (pasted) await viewer.showText(pasted.text);
  };

  const actions = menuActions({
    open: pick,
    reload: () => viewer.reload(),
    print,
    find: () => editing.find() || finder.open(),
    paste,
    toggleOutline: () => {
      update({ outline: !prefs.outline });
      outline.show(prefs.outline);
      relayoutSoon();
    },
    setTheme: (theme) => {
      update({ theme });
      return viewer.reload();
    },
    setWidth: (width) => update({ width }),
    zoom: (direction) => viewer.zoom(direction),
    save: () => editing.save(),
    toggleEdit: () => editing.toggle(),
    undo: () => editing.undo(),
    redo: () => editing.redo(),
    toggleAutosave: () => editing.toggleAutosave(),
    toggleSyncCursor: () => editing.toggleSyncCursor(),
  });

  const openPending = async () => {
    const path = await initialFile();
    if (path) await open_(path);
  };

  byId("open").addEventListener("click", () => void pick());
  window.addEventListener("resize", relayoutSoon);

  let lastWheelZoom = 0;
  byId("scroller").addEventListener(
    "wheel",
    (e) => {
      if (!e.ctrlKey && !e.metaKey) return;
      e.preventDefault();
      const now = performance.now();
      if (now - lastWheelZoom < WHEEL_ZOOM_INTERVAL_MS) return;
      lastWheelZoom = now;
      void viewer.zoom(e.deltaY < 0 ? "in" : "out");
    },
    { passive: false },
  );

  byId("pdf").addEventListener("click", async (e) => {
    const path = viewer.currentPath;
    const wrapper = (e.target as HTMLElement).closest<HTMLElement>(".page");
    if (!e.metaKey || !wrapper || !hasJumpableSource(path)) return;
    const click = pageClick(wrapper, e.clientX, e.clientY);
    if (!click) return;
    e.preventDefault();
    // 編集中で、開いているファイルのソースなら、エディタのカーソルを動かす
    if (editing.active && (await editing.revealSource(click.page, click.x, click.y))) return;
    try {
      const where = await jumpToSource(path, click.page, click.x, click.y);
      toast(where ? `Opened ${where}` : "No source found for this position");
    } catch (error) {
      toast(String(error), 5000);
    }
  });

  document.addEventListener("click", (e) => {
    const href = (e.target as HTMLElement).closest("a")?.getAttribute("href");
    if (!href || href.startsWith("#")) return;
    e.preventDefault();
    if (/^https?:\/\//.test(href)) {
      void openUrl(href);
      return;
    }
    const base = viewer.currentPath;
    const target = base && resolveLink(base, href);
    if (target && isSupported(target)) void open_(target);
  });

  await listen<string>(MENU_EVENT, (e) => void actions[e.payload]?.());
  await listen(CHANGED_EVENT, async () => {
    if (!(await editing.diskChanged())) reloadSoon();
  });

  // 未保存の変更があるウィンドウを閉じる／終了するときは、確認する
  const confirmThenDestroy = async () => {
    if (await editing.confirmDiscardOrSave()) await getCurrentWindow().destroy();
  };
  await getCurrentWindow().onCloseRequested(async (e) => {
    if (!editing.dirty) return;
    e.preventDefault();
    await confirmThenDestroy();
  });
  await listen(QUIT_EVENT, () => void confirmThenDestroy());
  await listen(OPEN_EVENT, () => void openPending());
  await getCurrentWebview().onDragDropEvent((e) => {
    if (e.payload.type !== "drop") return;
    const dropped = e.payload.paths.find(isSupported);
    if (dropped) void open_(dropped);
  });

  await openPending();
}

window.addEventListener("DOMContentLoaded", () => {
  // 起動中の例外で、画面が白紙のまま何も分からなくならないように
  main().catch((e) => {
    const error = document.getElementById("error");
    if (error) {
      error.textContent = `Failed to start: ${String(e)}`;
      error.hidden = false;
    }
    console.error(e);
  });
});
