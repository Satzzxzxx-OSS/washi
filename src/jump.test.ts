import { describe, expect, it } from "vitest";
import { hasJumpableSource, pageClick } from "./jump";

const wrapper = (page: string, scale: string, left = 100, top = 50) => ({
  dataset: { page, scale },
  getBoundingClientRect: () => ({ left, top }) as DOMRect,
});

describe("pageClick", () => {
  it("converts a screen position to points on the page", () => {
    expect(pageClick(wrapper("2", "2"), 140, 90)).toEqual({ page: 2, x: 20, y: 20 });
  });

  it("returns null when the page metadata is missing", () => {
    expect(pageClick(wrapper("", "2"), 0, 0)).toBeNull();
    expect(pageClick(wrapper("1", "0"), 0, 0)).toBeNull();
    expect(pageClick({ dataset: {}, getBoundingClientRect: () => ({ left: 0, top: 0 }) as DOMRect }, 0, 0)).toBeNull();
  });
});

describe("hasJumpableSource", () => {
  it("accepts Typst and LaTeX sources only", () => {
    expect(hasJumpableSource("/a/b.typ")).toBe(true);
    expect(hasJumpableSource("/a/b.TEX")).toBe(true);
    expect(hasJumpableSource("/a/b.pdf")).toBe(false);
    expect(hasJumpableSource("/a/b.md")).toBe(false);
    expect(hasJumpableSource(null)).toBe(false);
  });
});
