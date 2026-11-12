/**
 * The one-file export (tools/export-html.ts): the page loads nothing but itself, and what it
 * embeds is the engine, the worklet and the dictionary the app would otherwise fetch.
 */
import { describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { exportHtml } from "../tools/export-html.ts";

const here = (p: string) => fileURLToPath(new URL(p, import.meta.url));

describe("the app as one HTML file", () => {
  test("it loads no other file, and embeds the engine, the worklet, the dictionary and the fonts", async () => {
    const { html, embedded } = await exportHtml();
    // The page without what its <script> elements hold (React's own code has '<link rel="stylesheet"'
    // in a string). Checks are on booleans: a failed match on the page would print 4 MB of it.
    const shell = html.replace(/(<script\b[^>]*>)[\s\S]*?<\/script>/g, "$1</script>");
    expect(shell.length).toBeLessThan(400_000);
    // nothing fetched by a tag or by the stylesheet (the one link left is the icon, a data: URL)
    expect(/<script[^>]+\ssrc=/.test(shell)).toBe(false);
    expect(/<link[^>]+rel="(?:stylesheet|modulepreload)"/.test(shell)).toBe(false);
    for (const href of shell.matchAll(/<link[^>]+href="([^"]+)"/g)) expect(href[1].startsWith("data:")).toBe(true);
    const css = /<style>([\s\S]*?)<\/style>/.exec(shell)![1];
    for (const url of css.matchAll(/url\(\s*["']?([^"')]+)/g)) expect(url[1].startsWith("data:")).toBe(true);
    expect([...css.matchAll(/url\(data:font\//g)].length).toBe(4);
    // the app's script is in the page whole: one module, not a loader for chunks
    expect([...shell.matchAll(/<script type="module">/g)].length).toBe(1);
    expect(embedded.script).toBeGreaterThan(500_000); // three.js is in it (the 3D head is a lazy chunk otherwise)
    // the data blocks decode to what the app would have downloaded
    const block = (id: string) => Buffer.from(new RegExp(`id="${id}">([^<]*)<`).exec(html)![1], "base64");
    expect(block("chimera-wasm").equals(readFileSync(here("../src/wasm/chimera.wasm")))).toBe(true);
    expect(block("chimera-dictionary").equals(readFileSync(here("../public/data/cmudict.txt.gz")))).toBe(true);
    const worklet = block("chimera-worklet").toString("utf8");
    expect(worklet.includes("registerProcessor")).toBe(true);
    expect(/^\s*import\s/m.test(worklet)).toBe(false); // one file: a data: URL module can't import a neighbour
    // and the page hands them over under the names the app asks for
    const wasmName = Object.keys(embedded).find((k) => k.endsWith(".wasm"))!;
    const workletName = Object.keys(embedded).find((k) => /^processor-.*\.js$/.test(k))!;
    for (const name of [wasmName, workletName, "cmudict.txt.gz"]) expect(html.includes(JSON.stringify(name))).toBe(true);
    expect(html.includes(`/assets/${wasmName}`)).toBe(true); // the app's script still asks by that URL
    // small enough to mail, or to publish where a page may be 16 MB
    expect(Buffer.byteLength(html)).toBeLessThan(6_000_000);
  }, 60_000);
});
