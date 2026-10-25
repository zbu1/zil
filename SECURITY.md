# Security

zil is built to run **untrusted code**: a script from a stranger, pasted into a page or an app that embeds zil, shouldn't be able to harm the person running it or the app hosting it.

## Reporting

Please report vulnerabilities privately through GitHub's "Report a vulnerability" (Security tab), not in a public issue.

## The model

A `Session` touches the outside world only through its `Host`. A `Host` method the embedder doesn't implement is denied, and a denied call is an ordinary zil error. `NullHost` implements nothing: no output, no files, no network, a clock stuck at 1970 and fixed random numbers. It is the default host on every target; the CLI installs its own `NativeHost` (the whole machine).

Resources are bounded by `Session::limits` (`Limits`): eval steps, wall time, an interrupt flag, call depth, the size of one list/map/string, the bits in one exact number, and bytes printed. Hitting one is an `Error` whose `limit` names it, so a host can tell "too much" from a bug in the script.

## In scope

- **Escaping to the host.** A script reaching files, processes, the network, the environment or the clock beyond what its `Host` grants.
- **Denial of service.** A script that hangs, exhausts memory or overflows the stack despite limits being set, or crashes the process (a panic, an abort) on any input. The lexer, parser and evaluator are fuzzed (`fuzz/`), and `tests/smash.rs` calls every builtin with edge arguments.
- **Cross-session leaks.** One session seeing or changing another's variables, units, rates, random numbers or permissions. Sessions on the same thread share only immutable data.

## Out of scope

- **Side channels:** timing, cache or memory-usage observations.
- **Default limits.** They are generous (no step or time cap); embedders running untrusted code must set `limits`.
- **What a host grants.** If a host implements `http_get`, scripts can GET any URL it allows, and currency rates, translation and zip codes go online on their own. A host that wants no network leaves `http_get` unimplemented; filtering URLs belongs to the host.
- **Output content.** Printed text may contain terminal escapes (zil colors its own output); a host showing it somewhere escapes matter should sanitize it.
- **Regex size.** Regex literals compile in the parser, outside any session, bounded only by the regex crate's default 10 MB limit.

## Known gaps

- The browser sandbox (`docs/`) sets only a 5 s time limit and a 1 MiB output cap per run; a single slow builtin call isn't checked mid-call, so killing the worker after 10 s stays the backstop.
