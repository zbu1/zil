// The JS Session API under Node: `pnpm test:wasm` builds the bindings into target/pkg-test and runs this.
import assert from "node:assert";
import { readFileSync } from "node:fs";
import { initSync, Session, ZilError } from "../../../target/pkg-test/zil.js";

initSync({ module: readFileSync(new URL("../../../target/pkg-test/zil_bg.wasm", import.meta.url)) });
const printed = [];
const s = new Session({ onPrint: t => printed.push(t), limits: { steps: 100000 } });
assert.strictEqual(s.eval("2 + 3"), 5);
assert.strictEqual(s.eval("2**70"), 2n ** 70n);
assert.strictEqual(s.eval("2**60"), 2n ** 60n);
// Decimals are exact in zil, so they come back as fractions.
assert.strictEqual(s.eval("1.5").value, 1.5);
assert.strictEqual(String(s.eval("1.5")), "1.5");
assert.strictEqual(s.eval("sqrt(2)"), Math.SQRT2);
assert.strictEqual(s.eval('"hi"'), "hi");
assert.strictEqual(s.eval("nil"), null);
const f = s.eval("7/2");
assert.deepStrictEqual([f.type, f.num, f.den, f.value], ["frac", 7n, 2n, 3.5]);
const q = s.eval("3 km/h");
assert.deepStrictEqual([q.type, q.value, q.unit, String(q)], ["quantity", 3, "km/h", "3 km/h"]);
const m = s.eval("{a: 1, b: [1, set(2)]}");
assert.ok(m instanceof Map && m.get("b")[1] instanceof Set);
const d = s.eval('date("2026-10-06T14:30:00[Europe/Paris]")');
assert.deepStrictEqual([d.type, d.tz, d.epochMs], ["date", "Europe/Paris", Date.UTC(2026, 9, 6, 12, 30)]);
s.eval('print("out")');
assert.deepStrictEqual(printed, ["out"]);
s.set("x", 20);
s.set("big", 2n ** 80n);
s.set("o", { k: [1, "s"] });
s.set("w", q);
assert.strictEqual(s.eval("x * 2 + 2"), 42);
assert.strictEqual(s.eval("big == 2**80"), true);
assert.strictEqual(s.eval('o.k[1]'), "s");
assert.strictEqual(String(s.eval("w to m/s")), String(s.eval("3 km/h to m/s")));
assert.strictEqual(s.get("nope"), undefined);
const double = s.eval("double = |x| x * 2");
assert.strictEqual(s.call(double, [21]), 42);
assert.strictEqual(String(s.get("double")), String(double));
assert.strictEqual(s.call(s.get("max"), [1, 3]), 3);
assert.strictEqual(s.call(s.eval("[|x| x + 1]")[0], [1]), 2);
assert.throws(() => s.call(double, []), ZilError);
assert.throws(() => s.set("f", double), TypeError);
assert.throws(() => s.set("bad", { type: "quantity", value: 1, unit: "zzz" }), TypeError);
try { s.eval("1 +\n  nope"); assert.fail(); } catch (e) {
  assert.ok(e instanceof ZilError); assert.deepStrictEqual([e.message, e.line, e.column, e.limit], ["undefined variable `nope`", 2, 3, undefined]); assert.ok(e.render().includes("nope"));
}
assert.throws(() => s.eval("while true {}"), e => e.limit === "steps");
const p = new Session({ precision: 10 });
assert.strictEqual(String(p.eval("pi * 1 m")), "3.141592654 m");
assert.throws(() => new Session({ precision: 0 }), TypeError);
console.log("session.mjs: ok");
