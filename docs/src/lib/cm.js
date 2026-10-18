/*! CodeMirror and Lezer (bundled into the sandbox), https://codemirror.net
 * Copyright (C) 2018-2024 by Marijn Haverbeke <marijn@haverbeke.berlin> and others. MIT License:
 * Permission is hereby granted, free of charge, to any person obtaining a copy of this software and associated documentation files (the "Software"),
 * to deal in the Software without restriction, including without limitation the rights to use, copy, modify, merge, publish, distribute, sublicense,
 * and/or sell copies of the Software, and to permit persons to whom the Software is furnished to do so, subject to the following conditions:
 * The above copyright notice and this permission notice shall be included in all copies or substantial portions of the Software.
 * THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
 * FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
 * LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS
 * IN THE SOFTWARE.
 */
// zil for CodeMirror: highlighting, completion, hover docs, signature hints and inline results, from the wasm's vocabulary.
import { autocompletion, closeBrackets, closeBracketsKeymap, completionKeymap, acceptCompletion } from "@codemirror/autocomplete";
import { defaultKeymap, history, historyKeymap, indentLess, insertTab } from "@codemirror/commands";
import { HighlightStyle, StreamLanguage, bracketMatching, indentUnit, syntaxHighlighting } from "@codemirror/language";
import { Compartment, EditorState, Prec, RangeSetBuilder, StateEffect, StateField } from "@codemirror/state";
import { Decoration, EditorView, GutterMarker, WidgetType, drawSelection, gutterLineClass, hoverTooltip, keymap, lineNumbers, showTooltip } from "@codemirror/view";
import { tags } from "@lezer/highlight";
import { KEYWORDS, call, tokenAt, vocab } from "./client.js";
import "./cm.css";

export { EditorState, EditorView, Prec, keymap, lineNumbers };

const language = () =>
  StreamLanguage.define({
    name: "zil",
    token(stream) {
      if (stream.eatSpace()) return null;
      const t = tokenAt(stream.string, stream.pos);
      if (!t) {
        stream.next();
        return null;
      }
      stream.pos = t[1];
      return t[0];
    },
    languageData: { commentTokens: { line: "#" }, closeBrackets: { brackets: ["(", "[", "{", '"'] } },
    tokenTable: { result: tags.meta, call: tags.function(tags.variableName), unit: tags.unit, const: tags.constant(tags.variableName), op: tags.operator },
  });

const style = HighlightStyle.define([
  { tag: tags.keyword, color: "var(--k)" },
  { tag: tags.operator, color: "var(--k)" },
  { tag: tags.string, color: "var(--s)" },
  { tag: [tags.number, tags.constant(tags.variableName)], color: "var(--n)" },
  { tag: tags.function(tags.variableName), color: "var(--f)" },
  { tag: tags.unit, color: "var(--u)" },
  { tag: [tags.comment, tags.meta], color: "var(--c)", fontStyle: "italic" },
]);

/** Highlighting re-reads `vocab` only when the language is rebuilt: `relearn` does that once the wasm has sent it. */
const lang = new Compartment();
export const relearn = (view) => view.dispatch({ effects: lang.reconfigure(language()) });

const el = (tag, cls, text) => Object.assign(document.createElement(tag), { className: cls ?? "", textContent: text ?? "" });

/** Docs for a name, as a tooltip or completion panel; `onHelp` gets the name when its "help" link is clicked. */
function docs(name, onHelp) {
  const f = vocab.fns.get(name),
    u = vocab.units.get(name);
  if (!f && !u) return null;
  const dom = el("div", "zil-doc");
  dom.append(el("code", "", f ? f.sig : u.names.replaceAll(" ", ", ")), el("p", "", f ? f.desc : u.desc));
  const foot = el("footer", "", f ? f.module : "unit");
  if (onHelp) {
    const a = el("button", "link", `help(${f ? name : JSON.stringify(name)})`);
    a.addEventListener("mousedown", (e) => (e.preventDefault(), onHelp(name)));
    foot.append(" · ", a);
  }
  dom.append(foot);
  return dom;
}

/** Whether `before` (a line up to the cursor) ends inside a string or comment, and the string's text so far if a string. */
function inside(before) {
  let at = 0,
    last = null;
  while (at < before.length) {
    if (/\s/.test(before[at])) {
      at++;
      continue;
    }
    const t = tokenAt(before, at);
    last = t ? [t[0], at, t[1]] : null;
    at = t ? t[1] : at + 1;
  }
  if (!last || last[2] !== before.length) return null;
  const text = before.slice(last[1]);
  if (last[0] === "comment" || last[0] === "result") return { comment: true };
  if (last[0] === "string" && !/^r?"(?:\\.|[^"\\])*"$/.test(text)) return { string: text };
  return null;
}

let options = null;
function allOptions() {
  if (options?.size === vocab.fns.size + vocab.units.size) return options;
  const fns = [...vocab.fns.values()].map((f) => ({ label: f.name, type: "function", detail: f.module, info: () => docs(f.name), boost: 1 }));
  const units = [...vocab.units.values()].map((u) => ({ label: u.name, type: "unit", detail: "unit", info: () => docs(u.name) }));
  const consts = [...vocab.consts].map((c) => ({ label: c, type: "constant" }));
  const words = KEYWORDS.map((k) => ({ label: k, type: "keyword", boost: -1 }));
  const topics = [...vocab.modules].map(([path, about]) => ({ label: path, type: "namespace", detail: about }));
  const methods = fns.map((o) => ({ ...o, type: "method" }));
  options = { size: vocab.fns.size + vocab.units.size, all: [...fns, ...units, ...consts, ...words], methods, topics: [...topics, ...fns] };
  return options;
}

/** Names this document assigns, so your own variables complete too. */
function locals(doc, skip) {
  const seen = new Set();
  for (const m of doc.matchAll(/(?:^|[;\n|,(]\s*)([A-Za-z_]\w*)\s*(?:=(?!=)|\bin\b)|\bfor\s+([A-Za-z_]\w*)/g)) {
    const n = m[1] ?? m[2];
    if (n !== skip) seen.add(n);
  }
  return [...seen].map((n) => ({ label: n, type: "variable", boost: 2 }));
}

async function complete(ctx) {
  const line = ctx.state.doc.lineAt(ctx.pos);
  const before = line.text.slice(0, ctx.pos - line.from);
  const where = inside(before);
  if (where?.comment) return null;
  if (where?.string !== undefined) {
    // `help("` completes module paths and function names.
    if (!/help\(\s*r?"[^"]*$/.test(before)) return null;
    const word = ctx.matchBefore(/[\w.]*/);
    return { from: word.from, options: allOptions().topics, validFor: /^[\w.]*$/ };
  }
  const word = ctx.matchBefore(/[A-Za-z_ΩµμΩ][\wΩµμΩ]*/) ?? { from: ctx.pos, to: ctx.pos, text: "" };
  const dot = before[word.from - line.from - 1] === ".";
  // After `to`, the units the left side converts to, common ones first; after a quantity and a space, `to`. Only the wasm
  // knows units' kinds and the session's variables.
  const head = before.slice(0, word.from - line.from);
  if (/\bto\s+$/.test(head) || (!word.text && / $/.test(head))) {
    const hits = (await call("complete", head).catch(() => "")).split("\n").filter(Boolean);
    if (hits.length)
      return {
        from: word.from,
        options: hits.map((h, i) => ({ label: h, type: h === "to" ? "keyword" : "unit", info: () => docs(h), boost: 99 - Math.min(i, 198) })),
        validFor: /^[\wΩµμΩ]*$/,
      };
  }
  if (!word.text && !dot && !ctx.explicit) return null;
  const opts = allOptions();
  return {
    from: word.from,
    options: dot ? opts.methods : [...locals(ctx.state.doc.toString(), word.text), ...opts.all],
    validFor: /^[\wΩµμΩ]*$/,
  };
}

const hover = (onHelp) =>
  hoverTooltip((view, pos) => {
    const w = view.state.wordAt(pos);
    if (!w) return null;
    const dom = docs(view.state.sliceDoc(w.from, w.to), onHelp);
    return dom && { pos: w.from, end: w.to, above: true, create: () => ({ dom }) };
  });

/** The call the cursor is inside: its function and which argument, counting `x` in `x.f(y)` as the first. */
// ponytail: counts brackets and commas inside strings too; a real parse if that misleads.
function callAt(state) {
  const pos = state.selection.main.head,
    from = Math.max(0, pos - 2000);
  const text = state.sliceDoc(from, pos);
  let depth = 0,
    commas = 0;
  for (let i = text.length - 1; i >= 0; i--) {
    const c = text[i];
    if (")]}".includes(c)) depth++;
    else if ("([{".includes(c)) {
      if (depth-- > 0) continue;
      if (c !== "(") return null;
      const m = /([A-Za-z_]\w*)$/.exec(text.slice(0, i));
      const f = m && vocab.fns.get(m[1]);
      if (!f) return null;
      const method = text[i - m[1].length - 1] === ".";
      return { pos: from + i - m[1].length, f, arg: commas + (method ? 1 : 0) };
    } else if (c === "," && depth === 0) commas++;
  }
  return null;
}

/** A signature with the current argument in bold, every form of it. */
function signature(f, arg) {
  const dom = el("div", "zil-sig");
  for (const form of f.sig.split(" / ")) {
    const m = /^([^(]*)\((.*)\)(.*)$/.exec(form);
    const row = el("div");
    if (!m) {
      row.textContent = form;
      dom.append(row);
      continue;
    }
    const params = m[2] ? m[2].split(/,\s*(?![^\[(]*[\])])/) : [];
    // Past the last parameter, only a variadic `...` one is still being filled.
    const n = arg < params.length ? arg : params.at(-1)?.includes("...") ? params.length - 1 : -1;
    row.append(m[1] + "(");
    params.forEach((p, i) => {
      if (i) row.append(", ");
      row.append(i === n ? el("b", "", p) : p);
    });
    row.append(")" + m[3]);
    dom.append(row);
  }
  dom.append(el("p", "", f.desc));
  return dom;
}

const signatures = StateField.define({
  create: () => null,
  update(tip, tr) {
    if (!tr.docChanged && !tr.selection && !tr.effects.length) return tip;
    if (!tr.state.selection.main.empty) return null;
    const call = callAt(tr.state);
    return call && { pos: call.pos, above: true, create: () => ({ dom: signature(call.f, call.arg) }) };
  },
  provide: (f) => showTooltip.from(f),
});

const bad = (text) => text.startsWith("error: ");

/** A line's value: beside it, or under it if it's several lines, like the docs' built examples (ec.config.mjs). */
class Result extends WidgetType {
  constructor(text) {
    super();
    this.text = text;
  }
  eq(o) {
    return o.text === this.text;
  }
  toDOM() {
    if (this.text.includes("\n")) return el("div", bad(this.text) ? "zil-out bad" : "zil-out", this.text);
    const s = el("span", bad(this.text) ? "zil-inline bad" : "zil-inline", this.text);
    s.title = this.text;
    return s;
  }
}

const errorLine = new (class extends GutterMarker {
  elementClass = "zil-err";
})();

/** Each line's value beside it, from the wasm's `run` or `notebook`: `[[line, text], ...]`, errors as `error: ...`, marked in the gutter. */
export const setResults = StateEffect.define();
const results = StateField.define({
  create: () => Decoration.none,
  update(deco, tr) {
    for (const e of tr.effects) {
      if (!e.is(setResults)) continue;
      const doc = tr.state.doc;
      const widgets = e.value
        .filter(([line]) => line < doc.lines)
        .map(([line, text]) => {
          const at = doc.line(line + 1).to;
          return Decoration.widget({ widget: new Result(text), side: 1, block: text.includes("\n") }).range(at);
        });
      return Decoration.set(widgets, true);
    }
    if (!tr.docChanged) return deco;
    // Keep each value at the end of its own line, not carried along by text typed where it sits (like Enter at the line's end).
    const kept = [];
    deco.between(0, tr.startState.doc.length, (from, _, d) => {
      kept.push(Decoration.widget(d.spec).range(tr.state.doc.lineAt(tr.changes.mapPos(from, -1)).to));
    });
    return Decoration.set(kept, true);
  },
  provide: (f) => [
    EditorView.decorations.from(f),
    gutterLineClass.compute([f], (state) => {
      const b = new RangeSetBuilder();
      state.field(f).between(0, state.doc.length, (from, _, d) => {
        if (bad(d.spec.widget.text)) b.add(state.doc.lineAt(from).from, state.doc.lineAt(from).from, errorLine);
      });
      return b.finish();
    }),
  ],
});

/** Tab accepts a suggestion, else indents; Escape then Tab leaves the editor, as CodeMirror does. */
const tabs = keymap.of([
  { key: "Tab", run: (v) => acceptCompletion(v) || insertTab(v) },
  { key: "Shift-Tab", run: indentLess },
]);

/** Everything the sandbox's editors share; `onHelp(name)` is where hover docs' help links go. */
export function zil({ onHelp }) {
  return [
    lang.of(language()),
    syntaxHighlighting(style),
    indentUnit.of("  "),
    EditorState.tabSize.of(2),
    history(),
    drawSelection(),
    bracketMatching(),
    closeBrackets(),
    autocompletion({ override: [complete], icons: true, activateOnTyping: true }),
    hover(onHelp),
    signatures,
    results,
    Prec.high(tabs),
    keymap.of([...closeBracketsKeymap, ...completionKeymap, ...historyKeymap, ...defaultKeymap]),
    EditorView.contentAttributes.of({ spellcheck: "false", autocapitalize: "off", autocorrect: "off" }),
  ];
}
