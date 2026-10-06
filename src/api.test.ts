import { describe, expect, it } from "vitest";
import { decodeOutput } from "./api";

describe("decodeOutput", () => {
  it("decodes html with multibyte text", () => {
    const body = new TextEncoder().encode("<p>和紙</p>");
    const wire = new Uint8Array([0, ...body]);
    expect(decodeOutput(wire)).toEqual({ kind: "html", html: "<p>和紙</p>" });
  });

  it("decodes pdf bytes without the tag", () => {
    const out = decodeOutput(new Uint8Array([1, 37, 80, 68, 70]));
    expect(out.kind).toBe("pdf");
    expect(out.kind === "pdf" && [...out.bytes]).toEqual([37, 80, 68, 70]);
  });
});
