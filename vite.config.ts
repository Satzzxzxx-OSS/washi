import process from "node:process";
import { defineConfig } from "vite";

const host = process.env.TAURI_DEV_HOST;

export default defineConfig(() => ({
  clearScreen: false,
  build: {
    rollupOptions: {
      output: {
        // エディタ（CodeMirror）は ⌘E で初めて読む。起動時のチャンクに混ぜない
        manualChunks: (id: string) =>
          /node_modules\/(@codemirror|@lezer|codemirror-lang-typst|crelt|style-mod|w3c-keyname)\//.test(id)
            ? "vendor-cm"
            : undefined,
      },
    },
  },
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      ignored: ["**/src-tauri/**"],
    },
  },
}));
