import { describe, expect, it } from "vitest";
import { decodeBuffer, decodeOutput } from "./api";

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

describe("decodeBuffer", () => {
  const wire = (tag: number, diagnostics: unknown, body: Uint8Array) => {
    const json = new TextEncoder().encode(JSON.stringify(diagnostics));
    const header = new Uint8Array(5);
    header[0] = tag;
    new DataView(header.buffer).setUint32(1, json.length, false);
    return new Uint8Array([...header, ...json, ...body]);
  };
  const diagnostic = {
    file: null,
    line: 2,
    column: 3,
    end_line: 2,
    end_column: 9,
    severity: "error",
    message: "エラー",
    hints: ["try this"],
  };

  it("decodes an html render with its diagnostics", () => {
    const result = decodeBuffer(wire(0, [], new TextEncoder().encode("<p>和紙</p>")));
    expect(result).toEqual({ ok: true, output: { kind: "html", html: "<p>和紙</p>" }, diagnostics: [] });
  });

  it("decodes a pdf render, and turns the wire field names into camelCase", () => {
    const result = decodeBuffer(wire(1, [{ ...diagnostic, severity: "warning" }], new Uint8Array([37, 80])));
    expect(result.ok && result.output.kind === "pdf" && [...result.output.bytes]).toEqual([37, 80]);
    expect(result.diagnostics[0]).toEqual({
      file: null,
      line: 2,
      column: 3,
      endLine: 2,
      endColumn: 9,
      severity: "warning",
      message: "エラー",
      hints: ["try this"],
    });
  });

  it("decodes a failed render: the message and the diagnostics, no output", () => {
    const result = decodeBuffer(wire(2, [diagnostic], new TextEncoder().encode("コンパイルに失敗")));
    expect(result.ok).toBe(false);
    expect(!result.ok && result.message).toBe("コンパイルに失敗");
    expect(result.diagnostics).toHaveLength(1);
  });

  it("reads a view into a larger buffer (the wire is a subarray)", () => {
    const inner = wire(0, [], new TextEncoder().encode("x"));
    const padded = new Uint8Array([9, 9, 9, ...inner]).subarray(3);
    expect(decodeBuffer(padded)).toEqual({ ok: true, output: { kind: "html", html: "x" }, diagnostics: [] });
  });
});
