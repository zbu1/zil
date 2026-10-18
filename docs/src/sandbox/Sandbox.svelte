<!-- The sandbox: a REPL and a script editor on the wasm build, a searchable example catalog, and a first-visit tour. -->
<script>
  import { completionStatus } from "@codemirror/autocomplete";
  import { insertNewlineAndIndent } from "@codemirror/commands";
  import { highlightActiveLine, highlightActiveLineGutter, lineNumbers, placeholder } from "@codemirror/view";
  import { Switch, ToggleGroup } from "bits-ui";
  import { onMount } from "svelte";
  import { call, decode, encode, escape, generation, highlight, learn, stop, vocab, warm } from "../lib/client.js";
  import { EditorState, EditorView, Prec, keymap, relearn, setResults, zil } from "../lib/cm.js";
  import Catalog from "./Catalog.svelte";
  import Tour from "./Tour.svelte";
  import { store } from "./store.js";
  import "./sandbox.css";

  const START = `# zil: a calculator language with units, dates and exact math.
# Each line shows its value as you type. Change a number and watch.

trip = 42 km
trip to mi
trip / 65 mph to min
date("2026-12-25 18:30") to "Tokyo"
0.1 + 0.2 == 0.3

[3, 1, 2] |> sort |> map(|x| x ** 2)
`;
  const SAVED = "zil.sandbox",
    PAST = "zil.history",
    MODE = "zil.mode",
    PASTE = "zil.paste";
  const key = /Mac|iPhone|iPad/.test(navigator.platform) ? "⌘ ↵" : "Ctrl ↵";

  let mode = $state("editor");
  let paste = $state(store.get(PASTE) === "replace" ? "replace" : "append");
  let live = $state(store.get("zil.live") !== "false");
  let touring = $state(false);
  /** The catalog's data, once the wasm has sent it. */
  let sections = $state(null);
  /** Which catalog snippet was last used, until the script is edited. */
  let current = $state(null);

  function setMode(m) {
    mode = m;
    store.set(MODE, m);
  }

  // ── Script mode ──────────────────────────────────────────────────────────────

  let editorEl, editor;
  let output = $state(`<p class="note">Loading zil…</p>`);
  let status = $state(""),
    running = $state(false),
    shareLabel = $state("Share");
  const code = () => editor.state.doc.toString();

  let timer;
  function edited() {
    current = null;
    store.set(SAVED, code());
    clearTimeout(timer);
    if (live) timer = setTimeout(run, 350);
  }

  /** Replaces the whole script, undoably, and runs it. */
  function load(src) {
    editor.dispatch({ changes: { from: 0, to: editor.state.doc.length, insert: src }, selection: { anchor: 0 }, scrollIntoView: true });
    editor.focus();
    run();
  }

  /** Adds `src` on a new line after the cursor's line, so a snippet never wipes out what you wrote. */
  function insert(src) {
    const line = editor.state.doc.lineAt(editor.state.selection.main.head);
    const text = (line.text.trim() ? "\n" : "") + src;
    editor.dispatch({ changes: { from: line.to, insert: text }, selection: { anchor: line.to + text.length }, scrollIntoView: true });
    editor.focus();
    run();
  }

  let again = false;
  async function run() {
    if (running) return void (again = true);
    running = true;
    status = "Running";
    const t0 = performance.now();
    try {
      const [printed, result, error, lines] = await call("run", code());
      editor.dispatch({ effects: setResults.of(JSON.parse(lines)) });
      output =
        (printed ? `<pre class="printed">${printed}</pre>` : "") +
          (result ? `<div class="result"><pre>${result}</pre></div>` : "") +
          (error ? `<div class="error"><pre>${error}</pre></div>` : "") ||
        `<p class="note">Nothing printed. Each line's value shows beside it in the editor; anything you <code>print</code>, and the last line's value, shows here.</p>`;
      status = `${Math.max(1, Math.round(performance.now() - t0))} ms`;
    } catch (e) {
      output = `<div class="error"><pre>${escape(e.message)}</pre></div>`;
      status = "";
    } finally {
      running = false;
      if (again) {
        again = false;
        run();
      }
    }
  }

  function setLive(on) {
    live = on;
    store.set("zil.live", on);
    if (on) run();
  }

  async function share() {
    const url = `${location.origin}${location.pathname}#code=${encode(code())}`;
    history.replaceState(null, "", url);
    try {
      await navigator.clipboard.writeText(url);
      shareLabel = "Link copied";
    } catch {
      shareLabel = "Link in address bar";
    }
    setTimeout(() => (shareLabel = "Share"), 1600);
  }

  // ── REPL mode: one session in the worker, like the terminal REPL, with the prompt as the transcript's last line ──

  let logEl, inputEl, input;
  /** The transcript: each entry's HTML, appended to as its output arrives. */
  let entries = $state([]);
  let rstatus = $state(""),
    busy = $state(false);
  let past = [];
  try {
    past = JSON.parse(store.get(PAST)) ?? [];
  } catch {}
  let pos = past.length,
    draft = "";
  /** The worker generation whose session we greeted; another one means the session was lost. */
  let session = 0;

  const typed = () => input.state.doc.toString();
  function setInput(src) {
    input.dispatch({ changes: { from: 0, to: input.state.doc.length, insert: src }, selection: { anchor: src.length } });
  }
  function older() {
    if (pos === 0) return false;
    if (pos === past.length) draft = typed();
    setInput(past[--pos]);
    return true;
  }
  function newer() {
    if (pos === past.length) return false;
    setInput(++pos === past.length ? draft : past[pos]);
    return true;
  }
  /** Enter runs the input, as in a terminal (Tab takes a suggestion), unless a ( [ { is unclosed: then it starts a new line. */
  function enter(view) {
    if (busy) return true;
    call("repl_open", typed()).then((open) => (open ? insertNewlineAndIndent(view) : submit()));
    return true;
  }

  /** Adds an entry to the transcript, returning it (reactive) to append to. */
  function entry(html) {
    entries.push({ html });
    return entries[entries.length - 1];
  }
  const bottom = () => requestAnimationFrame(() => (logEl.scrollTop = logEl.scrollHeight));

  /** Greets a new session, saying so if one was lost: a runaway loop or a crash replaces the worker. */
  let greeting = null;
  const ensureSession = () =>
    (greeting ??= (async () => {
      await warm();
      if (session === generation) return;
      const lost = session !== 0;
      const banner = await call("repl_reset");
      session = generation;
      entry((lost ? `<p class="note">zil restarted, so earlier variables are gone.</p>` : "") + `<pre>${banner}</pre>`);
      bottom();
    })().finally(() => (greeting = null)));

  async function submit() {
    const src = typed().replace(/\s+$/, "");
    if (!src.trim()) return;
    if (src !== "clear history" && past.at(-1) !== src) past.push(src);
    past = src === "clear history" ? [] : past.slice(-200);
    pos = past.length;
    draft = "";
    store.set(PAST, JSON.stringify(past));
    setInput("");
    // The session forgets `_` and the `_n` results too.
    if (src === "clear history") return call("repl", src).catch(() => {});
    if (src === "clear") return (entries = []);
    if (src === "exit" || src === "quit") return reset();
    busy = true;
    // Greet first, so a "session lost" note lands above this input. A load failure shows up from the call below.
    await ensureSession().catch(() => {});
    const e = entry(`<div class="line"><span class="ps">›</span><pre>${highlight(src)}</pre></div>`);
    bottom();
    rstatus = "Running";
    const t0 = performance.now();
    try {
      const [printed, shown, error] = await call("repl", src);
      e.html += (printed ? `<pre>${printed}</pre>` : "") + (shown ? `<pre>${shown}</pre>` : "") + (error ? `<div class="error"><pre>${error}</pre></div>` : "");
      rstatus = `${Math.max(1, Math.round(performance.now() - t0))} ms`;
    } catch (err) {
      e.html += `<div class="error"><pre>${escape(err.message)}</pre></div>`;
      rstatus = "";
    } finally {
      busy = false;
      bottom();
      input.focus();
    }
  }

  async function reset() {
    entries = [];
    await warm();
    const banner = await call("repl_reset");
    session = generation;
    entry(`<pre>${banner}</pre>`);
    input.focus();
  }

  /** Runs `src` at the REPL prompt, switching to the REPL first. */
  async function repl(src) {
    if (busy) return;
    if (mode !== "repl") setMode("repl");
    await ensureSession().catch(() => {});
    setInput(src);
    return submit();
  }
  /** `help(...)` for a function, unit or module name, as you'd type it. */
  const helpFor = (name) => repl(vocab.fns.has(name) ? `help(${name})` : `help(${JSON.stringify(name)})`);

  // Help output links its names (`zil:help/upper`) and examples (`zil:run/...`).
  function follow(e) {
    const a = e.target.closest("a[data-zil]");
    if (!a) return;
    e.preventDefault();
    const [, kind, target] = /^zil:(\w+)\/(.*)$/.exec(a.dataset.zil) ?? [];
    if (kind === "help") helpFor(decodeURIComponent(target));
    else if (kind === "run") repl(decodeURIComponent(target));
  }
  // Clicking the transcript types at the prompt, unless it was to select text or follow a link.
  const focusInput = (e) => getSelection().isCollapsed && !e.target.closest("a, button") && input.focus();

  function switchMode(m) {
    if (!m) return; // a toggle group can be emptied by clicking the pressed item
    setMode(m);
    if (m === "repl") ensureSession().then(() => input.focus());
    else editor.focus();
  }

  // ── The catalog and the tour ─────────────────────────────────────────────────

  /** A catalog snippet: scripts open in Script mode; snippets run where you are, help pages always in the REPL. */
  async function pick({ id, src, file, inRepl }) {
    if (file) {
      if (mode === "editor" && paste === "append") insert(src);
      else setMode("editor"), load(src);
    } else if (mode === "repl" || inRepl) return repl(src);
    else (paste === "replace" ? load : insert)(src);
    current = id;
  }

  // The steps from `guide.rs`, flattened: {c, chapter, text, code}. The last one opens Script mode.
  const steps = call("tour").then((json) => JSON.parse(json).flatMap(([chapter, rows], c) => rows.map(([text, code]) => ({ c, chapter, text, code }))));
  function endTour() {
    touring = false;
    store.set("zil.toured", "1");
  }
  function openScript() {
    endTour();
    setMode("editor");
    load(START);
  }

  // ── Start: shared code, else what was here last time, else the welcome script ──

  onMount(() => {
    editor = new EditorView({
      parent: editorEl,
      state: EditorState.create({
        extensions: [
          lineNumbers(),
          highlightActiveLine(),
          highlightActiveLineGutter(),
          Prec.highest(keymap.of([{ key: "Mod-Enter", run: () => (run(), true) }])),
          zil({ onHelp: helpFor }),
          EditorView.contentAttributes.of({ "aria-label": "zil code" }),
          EditorView.updateListener.of((u) => u.docChanged && edited()),
        ],
      }),
    });
    const popup = (view) => completionStatus(view.state) === "active";
    input = new EditorView({
      parent: inputEl,
      state: EditorState.create({
        extensions: [
          Prec.highest(
            keymap.of([
              { key: "Enter", run: enter },
              { key: "Mod-Enter", run: () => (busy || submit(), true) },
              { key: "Shift-Enter", run: insertNewlineAndIndent },
              { key: "ArrowUp", run: (v) => !popup(v) && v.state.doc.lineAt(v.state.selection.main.head).number === 1 && older() },
              { key: "ArrowDown", run: (v) => !popup(v) && v.state.doc.lineAt(v.state.selection.main.head).number === v.state.doc.lines && newer() },
              { key: "Ctrl-l", run: () => ((entries = []), true) },
              { key: "Ctrl-c", run: (v) => busy && v.state.selection.main.empty && (stop(), true) },
            ]),
          ),
          zil({ onHelp: helpFor }),
          placeholder("try help(), or type a few letters and press Tab"),
          EditorView.lineWrapping,
          EditorView.contentAttributes.of({ "aria-label": "REPL input" }),
        ],
      }),
    });

    const shared = location.hash.match(/^#code=([\w-]*)/);
    editor.dispatch({ changes: { from: 0, insert: shared ? decode(shared[1]) : (store.get(SAVED) ?? START) } });
    // Mode: #repl links open the REPL, shared code opens the editor, else whichever was used last.
    setMode(location.hash !== "#repl" && (shared || store.get(MODE) === "editor") ? "editor" : "repl");
    if (!shared && !store.get("zil.toured")) touring = true;

    warm()
      .then(async () => {
        learn(await call("vocab"));
        relearn(editor);
        relearn(input);
        sections = JSON.parse(await call("examples"));
        run();
        if (mode === "repl") ensureSession().then(() => !touring && input.focus());
      })
      // Shown in both modes, since either may be the one open.
      .catch((e) => {
        output = `<div class="error"><pre>${escape(e.message)}</pre></div>`;
        entry(output);
      });
    return () => (editor.destroy(), input.destroy());
  });
</script>

<svelte:document onclick={follow} />

<div class="sandbox not-content" class:repl={mode === "repl"}>
  <aside class="rail pane" aria-label="Examples">
    <Catalog {sections} {current} bind:paste onPick={pick} onTour={() => (touring = true)} onPaste={(p) => store.set(PASTE, p)}>
      {#if touring}
        <Tour {steps} onRun={repl} onScript={openScript} onClose={endTour} />
      {/if}
    </Catalog>
  </aside>

  <section class="pane editor-only">
    <div class="head">
      {@render switcher()}
      <label class="toggle" title="Run as you type">
        <Switch.Root class="switch" checked={live} onCheckedChange={setLive}><Switch.Thumb class="thumb" /></Switch.Root>
        Live
      </label>
      <span class="spacer"></span>
      <button class="btn" onclick={share} title="Copy a link to this code">{shareLabel}</button>
      <button class="btn primary" onclick={() => (running ? stop() : run())}>{running ? "Stop" : "Run"} <kbd class="hide-sm">{key}</kbd></button>
    </div>
    <div class="editor" bind:this={editorEl}></div>
  </section>

  <section class="pane editor-only">
    <div class="head">
      <span class="title">Output</span>
      <span class="spacer"></span>
      <span class="status" class:busy={running} role="status">{status}</span>
    </div>
    <div class="out" aria-live="polite">{@html output}</div>
  </section>

  <section class="pane repl-only" aria-label="REPL">
    <div class="head">
      {@render switcher()}
      <span class="hide-sm keys"><kbd>Tab</kbd> complete · <kbd>↑</kbd> <kbd>↓</kbd> history · <kbd>Shift ↵</kbd> new line · <kbd>Ctrl L</kbd> clear</span>
      <span class="spacer"></span>
      <span class="status" class:busy role="status">{rstatus}</span>
      <button class="btn" onclick={reset} title="Forget every variable and start over">Reset</button>
    </div>
    <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
    <div class="log" bind:this={logEl} aria-live="polite" onclick={focusInput}>
      {#each entries as e}
        <div class="entry">{@html e.html}</div>
      {/each}
      <div class="line">
        <span class="ps" aria-hidden="true">›</span>
        <div class="input" bind:this={inputEl}></div>
      </div>
    </div>
  </section>

</div>

{#snippet switcher()}
  <ToggleGroup.Root class="modes" type="single" value={mode} onValueChange={switchMode} aria-label="Mode">
    <ToggleGroup.Item value="repl">REPL</ToggleGroup.Item>
    <ToggleGroup.Item value="editor">Script</ToggleGroup.Item>
  </ToggleGroup.Root>
{/snippet}
