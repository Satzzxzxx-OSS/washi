import { describe, expect, it } from "vitest";
import { DEFAULT_PREFS, loadPrefs, savePrefs } from "./prefs";

const memory = (initial?: string) => {
  let value = initial;
  return {
    getItem: () => value ?? null,
    setItem: (_: string, v: string) => void (value = v),
  };
};

describe("prefs", () => {
  it("returns defaults when nothing is stored", () => {
    expect(loadPrefs(memory())).toEqual(DEFAULT_PREFS);
  });

  it("round-trips valid values", () => {
    const store = memory();
    savePrefs({ theme: "dark", width: "wide", outline: true }, store);
    expect(loadPrefs(store)).toEqual({ theme: "dark", width: "wide", outline: true });
  });

  it("drops invalid values field by field", () => {
    const store = memory(JSON.stringify({ theme: "neon", width: "full", outline: "yes" }));
    expect(loadPrefs(store)).toEqual({ ...DEFAULT_PREFS, width: "full" });
  });

  it("survives corrupt JSON and missing or throwing storage", () => {
    expect(loadPrefs(memory("{not json"))).toEqual(DEFAULT_PREFS);
    expect(loadPrefs(null)).toEqual(DEFAULT_PREFS);
    const throwing = {
      getItem: () => {
        throw new Error("denied");
      },
      setItem: () => {
        throw new Error("denied");
      },
    };
    expect(loadPrefs(throwing)).toEqual(DEFAULT_PREFS);
    expect(() => savePrefs(DEFAULT_PREFS, throwing)).not.toThrow();
  });
});
