type Handler = (event: { event: string; id: number; payload: unknown }) => void;

const listeners = new Map<number, { event: string; handler: number }>();
const callbacks = new Map<number, Handler>();
let nextId = 1;

const params = new URLSearchParams(location.search);
const basename = (p: string) => p.split("/").pop() ?? p;

async function wire(name: string) {
  const res = await fetch(`/e2e/fixtures/${basename(name)}.wire`);
  if (!res.ok) throw new Error(`fixture が無い: ${name}`);
  return res.arrayBuffer();
}

const w = window as unknown as Record<string, unknown>;
w.__TAURI_INTERNALS__ = {
  metadata: {
    currentWindow: { label: "main" },
    currentWebview: { label: "main", windowLabel: "main" },
  },
  transformCallback(cb: Handler) {
    const id = nextId++;
    callbacks.set(id, cb);
    return id;
  },
  unregisterCallback(id: number) {
    callbacks.delete(id);
  },
  convertFileSrc: (p: string) => p,
  async invoke(cmd: string, args: Record<string, unknown> = {}) {
    switch (cmd) {
      case "supported_extensions":
        return ["md", "markdown", "mdown", "mmd", "mermaid", "typ", "tex", "latex", "pdf"];
      case "initial_file":
        return params.get("file");
      case "render":
        return wire(String(args.path));
      case "render_text":
        return wire(params.get("text") ?? "showcase.md");
      case "plugin:event|listen": {
        const id = nextId++;
        listeners.set(id, { event: String(args.event), handler: Number(args.handler) });
        return id;
      }
      case "plugin:clipboard-manager|read_text":
        return (w.__mockClipboard as string) ?? "";
      default:
        return null;
    }
  },
};

w.__emit = (event: string, payload?: unknown) => {
  for (const [id, l] of listeners) {
    if (l.event === event) callbacks.get(l.handler)?.({ event, id, payload });
  }
};
