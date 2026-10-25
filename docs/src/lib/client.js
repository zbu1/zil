// The page side of zil in the browser, shared by the sandbox and the docs' run buttons.

/** A run longer than this is a runaway loop: its worker is killed and replaced. */
export const LIMIT_MS = 10_000;

let worker, ready, nextId = 0;
const pending = new Map();
/** Counts workers started: a new one means anything kept in the old one, like the REPL's variables, is gone. */
export let generation = 0;

function spawn() {
  generation++;
  worker = new Worker(new URL("./worker.js", import.meta.url), { type: "module" });
  ready = new Promise((resolve, reject) => {
    // Most load failures are a cached zil.js or worker.js that doesn't match a newer zil_bg.wasm.
    const failed = (why) => {
      why = `couldn't load zil: ${why}\nReload the page without the cache (Ctrl+Shift+R, or ⌘⇧R on a Mac) to fetch a fresh copy.`;
      kill(why);
      reject(new Error(why));
    };
    worker.onmessage = ({ data }) => {
      if (data.ready) return resolve();
      if ("fail" in data) return failed(data.fail);
      const p = pending.get(data.id);
      if (!p) return;
      pending.delete(data.id);
      clearTimeout(p.timer);
      if ("crash" in data) {
        kill(`zil crashed: ${data.crash}`);
        p.reject(new Error(`zil crashed: ${data.crash}`));
      } else p.resolve(data.result);
    };
    worker.onerror = (e) => failed(e.message || "network error");
  });
}

/** Throws away the worker and fails whatever it was running. */
function kill(reason) {
  worker?.terminate();
  worker = null;
  for (const p of pending.values()) {
    clearTimeout(p.timer);
    p.reject(new Error(reason));
  }
  pending.clear();
}

/** Starts loading zil now, so the first run doesn't wait for the download. */
export function warm() {
  if (!worker) spawn();
  return ready;
}

/** Calls a `wasm.rs` export in the worker, like "run", "notebook" or "repl". */
export async function call(fn, src = "") {
  await warm();
  const id = nextId++;
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => kill(`stopped after ${LIMIT_MS / 1000} s; is there an endless loop?`), LIMIT_MS);
    pending.set(id, { resolve, reject, timer });
    worker.postMessage({ id, fn, src });
  });
}

/** Stops a run in progress. */
export const stop = () => kill("stopped");

export const KEYWORDS = ["fn", "if", "else", "while", "for", "in", "to", "of", "mod", "return", "break", "continue", "true", "false", "nil"];
const TOKEN = [
  /(?<result>^[ \t]*# → .*$)/,
  // Before comments, so `16#ff` is a number; durations and clock times, then the rest, like the lexer's.
  /(?<number>\d+#[0-9a-zA-Z_]+|\d+[dhms](?:\d+[dhms])+\b|\d{1,2}:\d{2}(?::\d{2})?(?:[ap]m)?|\d{1,2}[ap]m\b|0[xbo][0-9a-fA-F_]+|\d[\d_]*(?:\.\d[\d_]*)?(?:[eE][+-]?\d+)?)/,
  /(?<comment>#.*$)/,
  /(?<string>r?"(?:\\.|[^"\\])*"?)/,
  /(?<ident>[A-Za-z_ΩµμΩ][\wΩµμΩ]*)/,
  /(?<op>\|>|±|\+-|\.\.=?|[$€£])/,
]
  .map((r) => r.source)
  .join("|");
const ALL = new RegExp(TOKEN, "gm");
const STICKY = new RegExp(TOKEN, "my");

/** Names zil knows, from the wasm's `vocab()`; until `learn` gets them, only calls and keywords stand out. */
export const vocab = { fns: new Map(), units: new Map(), consts: new Set(), modules: new Map(), keywords: new Map() };
export function learn(json) {
  const v = JSON.parse(json);
  for (const [name, sig, desc, module] of v.fns) vocab.fns.set(name, { name, sig, desc, module });
  for (const [names, desc] of v.units) for (const n of names.split(" ")) vocab.units.set(n, { name: n, desc, names });
  for (const c of v.consts) vocab.consts.add(c);
  for (const [path, about] of v.modules) vocab.modules.set(path, about);
  for (const [name, form, desc] of v.keywords) vocab.keywords.set(name, { name, form, desc });
}

/** What a token is: `result`, `comment`, `string`, `number`, `keyword`, `call`, `unit`, `const`, `op`, or null for a plain name. */
function kind(m, src) {
  const k = Object.keys(m.groups).find((k) => m.groups[k] !== undefined);
  if (k !== "ident") return k;
  const w = m[0];
  if (KEYWORDS.includes(w)) return "keyword";
  // Called, or a method like `"hi".upper`, or piped to like `|> sum`.
  const before = src.slice(Math.max(0, m.index - 16), m.index);
  if (src[m.index + w.length] === "(" || (/(\.|\|>\s*)$/.test(before) && vocab.fns.has(w))) return "call";
  if (vocab.units.has(w)) return "unit";
  if (vocab.consts.has(w)) return "const";
  return vocab.fns.has(w) ? "call" : null;
}

/** The token starting at `at` in `line`, for CodeMirror: `[kind, end]`, or null if none starts there. */
export function tokenAt(line, at) {
  STICKY.lastIndex = at;
  const m = STICKY.exec(line);
  return m && m[0] ? [kind(m, line), at + m[0].length] : null;
}

const CLASS = {
  result: "zil-result",
  comment: "hljs-comment",
  string: "hljs-string",
  number: "hljs-number",
  keyword: "hljs-keyword",
  call: "hljs-title",
  unit: "hljs-type",
  const: "hljs-literal",
  op: "hljs-operator",
};

export const escape = (s) => s.replace(/[&<>]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;" })[c]);

/** zil source as HTML, using highlight.js class names, which theme.css colors. */
export function highlight(src) {
  let out = "",
    last = 0;
  for (const m of src.matchAll(ALL)) {
    const k = kind(m, src);
    if (!k) continue;
    out += escape(src.slice(last, m.index)) + `<span class="${CLASS[k]}">${escape(m[0])}</span>`;
    last = m.index + m[0].length;
  }
  return out + escape(src.slice(last));
}

/** Sandbox links carry the code in the URL hash as base64url UTF-8. */
export function encode(src) {
  const bin = String.fromCharCode(...new TextEncoder().encode(src));
  return btoa(bin).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}

export function decode(s) {
  const bin = atob(s.replace(/-/g, "+").replace(/_/g, "/"));
  return new TextDecoder().decode(Uint8Array.from(bin, (c) => c.charCodeAt(0)));
}
