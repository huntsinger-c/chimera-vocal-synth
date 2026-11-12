/**
 * The whole app as one HTML file: the UI, the engine (wasm), the AudioWorklet, the pronouncing
 * dictionary and the fonts. It runs opened from a disk (file://) or from any static host, with
 * no network and nothing beside it.
 * Usage: bun tools/export-html.ts [out/chimera.html]   (build the wasm first: bun run wasm)
 *
 * How: a Vite build with everything in one script (no lazy chunks) is written into the page.
 * What the app loads by URL at run time is embedded as data and handed over where it asks:
 *  - the wasm and the dictionary answer from the page, through a wrapper around fetch();
 *  - the worklet, which a browser only loads from a URL, loads from a data: or blob: URL;
 *  - the fonts are data: URLs in the CSS.
 * The app's own code is not changed, so the export is the app, not a port of it. Not included:
 * the duet shows (`?show=<id>` fetches public/shows, which is never published).
 */
import { build } from "vite";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { basename, dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");

const MIME: Record<string, string> = { otf: "font/otf", woff2: "font/woff2", wasm: "application/wasm", gz: "application/gzip" };
const mime = (name: string) => MIME[name.split(".").pop() ?? ""] ?? "application/octet-stream";
const b64 = (b: Uint8Array | string) => Buffer.from(b).toString("base64");
/** text that can sit inside a <script> element: nothing in it may read as the element's end, or
 *  as the start of an HTML comment (both only ever occur inside strings in minified code) */
const inScript = (code: string) => code.replace(/<\/script/gi, "<\\/script").replace(/<!--/g, "\\x3C!--");

/** Build the app and return the page as a string, with what was embedded (name → bytes). */
export async function exportHtml(): Promise<{ html: string; embedded: Record<string, number> }> {
  const result = await build({
    root,
    logLevel: "error",
    build: { write: false, sourcemap: false, cssCodeSplit: false, assetsInlineLimit: 0, rolldownOptions: { output: { codeSplitting: false } } },
  });
  const files = (Array.isArray(result) ? result : [result]).flatMap((r) => ("output" in r ? r.output : []));
  const chunks = files.filter((f) => f.type === "chunk");
  if (chunks.length !== 1) throw new Error(`export: expected the app in one script, got ${chunks.map((c) => c.fileName).join(", ")}`);
  const asset = (what: string, test: (name: string) => boolean) => {
    const hit = files.filter((f) => f.type === "asset" && test(f.fileName));
    if (hit.length !== 1) throw new Error(`export: expected one ${what} in the build, got ${hit.length}`);
    const f = hit[0] as { fileName: string; source: string | Uint8Array };
    return { name: basename(f.fileName), source: f.source };
  };
  const page = asset("page", (n) => n === "index.html");
  const style = asset("stylesheet", (n) => n.endsWith(".css"));
  const wasm = asset("wasm module", (n) => n.endsWith(".wasm"));
  const worklet = asset("worklet", (n) => /processor-.*\.js$/.test(n));
  const dict = { name: "cmudict.txt.gz", source: readFileSync(resolve(root, "public/data/cmudict.txt.gz")) };
  const embedded: Record<string, number> = { script: chunks[0].code.length, [wasm.name]: wasm.source.length, [worklet.name]: worklet.source.length, [dict.name]: dict.source.length };

  // fonts: every url() in the stylesheet is a file under public/fonts
  const css = String(style.source).replace(/url\(\s*(["']?)([^"')]+)\1\s*\)/g, (whole, _q, url: string) => {
    if (url.startsWith("data:")) return whole;
    const m = /^(?:\.{0,2}\/)*fonts\/([\w.-]+)$/.exec(url);
    if (!m) throw new Error(`export: the stylesheet loads ${url}, which this tool does not know how to embed`);
    const bytes = readFileSync(resolve(root, "public/fonts", m[1]));
    embedded[m[1]] = bytes.length;
    return `url(data:${mime(m[1])};base64,${b64(bytes)})`;
  });

  // what the page hands over in place of a download (runs before the app's script)
  const handover = `(() => {
  const block = (id) => document.getElementById(id).textContent;
  const bytes = (id) => Uint8Array.from(atob(block(id).trim()), (c) => c.charCodeAt(0));
  const leaf = (url) => String(url).split(/[?#]/)[0].split("/").pop();
  const embedded = { ${JSON.stringify(wasm.name)}: ["chimera-wasm", ${JSON.stringify(mime(wasm.name))}], ${JSON.stringify(dict.name)}: ["chimera-dictionary", ${JSON.stringify(mime(dict.name))}] };
  const download = window.fetch.bind(window);
  window.fetch = (input, init) => {
    const hit = embedded[leaf(input instanceof Request ? input.url : input)];
    return hit ? Promise.resolve(new Response(bytes(hit[0]), { headers: { "content-type": hit[1] } })) : download(input, init);
  };
  // (no AudioWorklet outside a secure context; the app reports that itself when it starts audio)
  if (window.AudioWorklet) {
    const add = AudioWorklet.prototype.addModule;
    // A page opened from a disk has no origin, and a blob: URL made there counts as another
    // site's script: the worklet fails to load ("a dependency or cross-origin script failed to
    // load"). A data: URL loads. A hosted page may have a policy that refuses data: scripts and
    // allows blob:, so each kind of page tries its likelier one first, then the other.
    const data = () => "data:text/javascript;charset=utf-8;base64," + block("chimera-worklet").trim();
    const blob = () => URL.createObjectURL(new Blob([bytes("chimera-worklet")], { type: "text/javascript" }));
    AudioWorklet.prototype.addModule = function (module, options) {
      if (leaf(module) !== ${JSON.stringify(worklet.name)}) return add.call(this, module, options);
      const [first, second] = self.origin === "null" ? [data, blob] : [blob, data];
      return add.call(this, first(), options).catch(() => add.call(this, second(), options));
    };
  }
})();`;

  let html = String(page.source)
    .replace(/\s*<link rel="(?:stylesheet|modulepreload)"[^>]*>/g, "")
    .replace(/\s*<script type="module"[^>]*><\/script>/g, "");
  if (/<script[^>]+src=|<link[^>]+rel="stylesheet"/.test(html)) throw new Error("export: the built page still loads a file");
  // (function replacements: a "$&" in the code must not be read as a pattern)
  html = html.replace("</head>", () => `  <style>${css}</style>\n  </head>`);
  html = html.replace(
    "</body>",
    () =>
      `  <script type="application/octet-stream" id="chimera-wasm">${b64(wasm.source)}</script>\n` +
      `    <script type="application/octet-stream" id="chimera-dictionary">${b64(dict.source)}</script>\n` +
      `    <script type="application/octet-stream" id="chimera-worklet">${b64(worklet.source)}</script>\n` +
      `    <script>${handover}</script>\n` +
      `    <script type="module">${inScript(chunks[0].code)}</script>\n  </body>`,
  );
  return { html, embedded };
}

if (import.meta.main) {
  const out = resolve(process.argv[2] ?? "out/chimera.html");
  const { html, embedded } = await exportHtml();
  mkdirSync(dirname(out), { recursive: true });
  writeFileSync(out, html);
  const kb = (n: number) => `${Math.round(n / 1024)} kB`;
  console.log(`wrote ${out}  ${kb(Buffer.byteLength(html))}`);
  for (const [name, n] of Object.entries(embedded)) console.log(`  ${name}  ${kb(n)}`);
}
