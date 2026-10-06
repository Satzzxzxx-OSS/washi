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
  renderText,
  supportedExtensions,
  watch,
} from "./api";
import { interpretPaste } from "./clipboard";
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
  renderText,
  async opened(path) {
    await getCurrentWindow().setTitle(`${basename(path)} — Washi`);
    await watch(path);
  },
  async pasted() {
    await getCurrentWindow().setTitle("貼り付け — Washi");
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
      filters: [{ name: "ドキュメント", extensions }],
    });
    if (typeof selected === "string") await viewer.load(selected);
  };

  const paste = async () => {
    const text = await readText().catch(() => "");
    const field = document.activeElement;
    if (field instanceof HTMLInputElement) {
      const end = field.value.length;
      field.setRangeText(text, field.selectionStart ?? end, field.selectionEnd ?? end, "end");
      return;
    }
    const pasted = interpretPaste(text, isSupported);
    if (pasted?.kind === "file") await viewer.load(pasted.path);
    else if (pasted) await viewer.showText(pasted.text);
  };

  const actions = menuActions({
    open: pick,
    reload: () => viewer.reload(),
    print,
    find: () => finder.open(),
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
  });

  const openPending = async () => {
    const path = await initialFile();
    if (path) await viewer.load(path);
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
    try {
      const where = await jumpToSource(path, click.page, click.x, click.y);
      toast(where ? `${where} を開きました` : "この位置に対応するソースは見つかりませんでした");
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
    if (target && isSupported(target)) void viewer.load(target);
  });

  await listen<string>(MENU_EVENT, (e) => void actions[e.payload]?.());
  await listen(CHANGED_EVENT, reloadSoon);
  await listen(OPEN_EVENT, () => void openPending());
  await getCurrentWebview().onDragDropEvent((e) => {
    if (e.payload.type !== "drop") return;
    const dropped = e.payload.paths.find(isSupported);
    if (dropped) void viewer.load(dropped);
  });

  await openPending();
}

window.addEventListener("DOMContentLoaded", () => void main());
