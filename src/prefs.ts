export const THEMES = ["system", "light", "dark"] as const;
export const WIDTHS = ["narrow", "wide", "full"] as const;

export type Theme = (typeof THEMES)[number];
export type Width = (typeof WIDTHS)[number];

export interface Prefs {
  theme: Theme;
  width: Width;
  outline: boolean;
}

export const DEFAULT_PREFS: Prefs = { theme: "system", width: "narrow", outline: false };

const KEY = "washi:prefs";

type Store = Pick<Storage, "getItem" | "setItem">;

function defaultStore(): Store | null {
  try {
    return localStorage;
  } catch {
    return null;
  }
}

const pick = <T extends string>(allowed: readonly T[], value: unknown, fallback: T): T =>
  allowed.includes(value as T) ? (value as T) : fallback;

export function loadPrefs(store: Store | null = defaultStore()): Prefs {
  try {
    const raw = store?.getItem(KEY);
    const parsed: Record<string, unknown> = raw ? JSON.parse(raw) : {};
    return {
      theme: pick(THEMES, parsed.theme, DEFAULT_PREFS.theme),
      width: pick(WIDTHS, parsed.width, DEFAULT_PREFS.width),
      outline: typeof parsed.outline === "boolean" ? parsed.outline : DEFAULT_PREFS.outline,
    };
  } catch {
    return { ...DEFAULT_PREFS };
  }
}

export function savePrefs(prefs: Prefs, store: Store | null = defaultStore()) {
  try {
    store?.setItem(KEY, JSON.stringify(prefs));
  } catch {
    return;
  }
}
