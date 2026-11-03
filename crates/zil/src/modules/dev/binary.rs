//! Executables: ELF, PE and Mach-O headers, tables and hardening, via the `object` crate.

use crate::ast::Radix;
use crate::host::Permission;
use crate::modules::{Call, Fail, Module};
use crate::signature::{Map, Or};
use crate::value::{Table, Value};
use iced_x86::{
    Code, Decoder, DecoderOptions, Encoder, FlowControl, Formatter, GasFormatter, Instruction, IntelFormatter, MemorySizeOptions, Mnemonic, OpKind, Register,
};
use indexmap::IndexMap;
use object::read::elf::{Dyn, ElfFile, FileHeader, ProgramHeader};
use object::read::macho::MachHeader;
use object::read::pe::{ImageNtHeaders, ImageOptionalHeader};
use object::{
    Architecture, Endianness, Object, ObjectKind, ObjectSection, ObjectSegment, ObjectSymbol, ObjectSymbolTable, RelocationFlags, RelocationTarget,
    SectionKind, elf, macho, pe,
};
use std::collections::HashMap;

pub const MODULE: Module = Module {
    name: "binaries",
    tags: &["binary"],
    permissions: &[Permission::FilesRead],
    about: "look inside compiled programs and libraries for Linux, Windows and macOS",
    #[rustfmt::skip]
    intro: &[
        ("", "\
Compiled programs and libraries are stored in one of three file formats: ELF on Linux and most other Unix systems, PE \
on Windows (`.exe` and `.dll` files), and Mach-O on macOS and iOS. Reading one gives a map of what the file is and how \
it was built, without tools like readelf, objdump or otool, and the help page lists the map's keys. Machine code for x86 \
and x64 can also be turned back into assembly."),
    ],
    fns: fns::FNS,
    call: fns::call,
    ..Module::EMPTY
};

#[zil_macros::builtins]
mod fns {
    use super::{assemble, binary as read, bits_opt, demangled, gadget_rows, hex, instructions, listing, read_path, row, segments, table, xref_rows};
    use crate::modules::Fail;
    use crate::signature::{As, Map, Or};
    use crate::value::{Table, Value};

    /// read a compiled program or library (ELF, PE or Mach-O) into a map of what it is, what's inside it and which security protections it was built with
    ///
    /// Takes a path or the file's bytes
    #[builtin(
        shown = [r#"binary("a.out").arch"#, r#"binary("a.out").imports.name"#, r#"binary(read_bytes("app.exe")).nx"#],
        see = ["read_bytes", "strings", "hexdump"],
        fields = [
            ("path", "str?", "the file it was read from, nil when given bytes"),
            ("format", "str", "\"elf\", \"pe\" or \"macho\""),
            ("arch", "str", "the CPU it's built for, like \"x86_64\" or \"aarch64\""),
            ("bits", "int", "32 or 64"),
            ("endian", "str", "byte order, \"little\" or \"big\""),
            ("entry", "int", "the address where the program starts running"),
            ("sections", "table", "the named pieces of the file, like `.text` for machine code and `.data` for globals. Columns name, addr, size, offset"),
            ("segments", "table", "how sections are loaded into memory. Columns name, addr, size (in memory), offset, file_size (in the file, with the rest zero-filled) and perm, whether it's readable, writable or executable, like \"r-x\""),
            ("symbols", "table", "the functions and variables the file defines. Columns name, addr, size, kind, global and demangled, the readable name of a Rust or C++ symbol"),
            ("stripped", "bool", "its symbol table was removed, leaving only the names shared libraries need, so `symbols` may be empty and code is found by address"),
            ("imports", "table", "the functions it borrows from shared libraries. Columns name, library"),
            ("exports", "table", "the functions it offers to other programs. Columns name, addr"),
            ("relocs", "table", "the places the loader or linker patches with an address, like a library function's slot in the GOT. Columns offset, kind (like \"jump_slot\"), symbol and addend"),
            ("got", "map?", "each library symbol's slot in the global offset table, where the loader writes its address. nil for PE and Mach-O"),
            ("plt", "map?", "each library function's stub in the procedure linkage table, the address the program calls it through. nil for PE and Mach-O"),
            ("pie", "bool", "it loads at a random address each run, so an attacker can't know where its code is"),
            ("nx", "bool", "memory holding data can't run as code, so bytes an attacker sneaks in can't execute"),
            ("canary", "bool", "functions check a guard value next to their locals before returning, catching most stack overflows"),
            ("relro", "str?", "\"full\", \"partial\" or \"none\": how much of the library address table is read-only after loading, so it can't be overwritten to redirect calls. nil for PE and Mach-O, which have no RELRO"),
        ]
    )]
    fn binary(file: Or<&str, &[u8]>) -> Result<As<Map>, Fail> {
        match file {
            Or::A(p) => read(&read_path(p)?, Some(p)).map(As::from),
            Or::B(b) => read(b, None).map(As::from),
        }
    }

    /// disassemble x86 or x64 machine code into assembly, laid out like `objdump -d`, or with {table: true} into a table of addr, bytes and text
    ///
    /// Each line has the address, the instruction's bytes in hex and its text, under a `<name>:` header where a function or section starts. Raw bytes are decoded from addr, 0 by default, as 64-bit code unless {bits: 32} or {bits: 16}. Given a symbol name instead of an address, the bytes are a whole executable and the function comes from its symbol table, as does `binary(path).disasm("main")`. `binary(path).disasm(addr)` decodes the executable's code from that address, for files stripped of symbols, and `binary(path).disasm()` all of its code. Intel syntax unless {syntax: "att"}
    #[builtin(
        examples = [r#"disasm(b"\x55\x48\x89\xe5\xc3")"#, r#"disasm(b"\x31\xc0\xc3", 0x401000, {syntax: "att"})"#, r#"disasm(b"\x55\x48\x89\xe5\xc3", nil, {table: true})"#],
        shown = [r#"binary("a.out").disasm("main")"#, r#"disasm(read_bytes("a.out"), "main")"#, r#"binary("a.out").disasm(nil, {table: true}).text"#],
        see = ["binary", "hexdump"],
    )]
    fn disasm(code: Or<&[u8], &Map>, at: Option<Or<i64, &str>>, opts: Option<&Map>) -> Result<Value, Fail> {
        if !opts.is_some_and(|m| m.borrow().get("table").is_some_and(Value::truthy)) {
            return Ok(Value::str(listing(&instructions(code, at, opts, true)?)));
        }
        let rows = instructions(code, at, opts, false)?
            .into_iter()
            .map(|(_, addr, bytes, text)| row([("addr", hex(addr)), ("bytes", Value::Bytes(bytes.into())), ("text", Value::str(text))]));
        Ok(table(rows)?.into())
    }

    /// assemble x86 or x64 instructions in Intel syntax into machine code, the reverse of disasm
    ///
    /// Instructions are separated by semicolons or new lines, and each gets its shortest encoding. Memory operands are written like `qword ptr [rbp + rax*8 - 0x10]`, and the size is needed only when no register operand gives it. Jumps and calls take the target address, and `[rip + 8]` is relative to the next instruction, so the code is assembled at addr, 0 by default, as 64-bit unless {bits: 32} or {bits: 16}. Labels and directives aren't supported
    #[builtin(
        examples = [r#"asm("xor eax, eax; push rax; ret")"#, r#"asm("mov qword ptr [rsp + 8], 0x41")"#, r#"asm("jmp 0x401000", 0x401000).disasm(0x401000)"#],
        see = ["disasm", "gadgets", "patch"]
    )]
    fn asm(text: &str, addr: Option<i64>, opts: Option<&Map>) -> Result<Value, Fail> {
        let bits = bits_opt(opts, 2)?;
        let ip = u64::try_from(addr.unwrap_or(0)).map_err(|_| Fail::Arg(1, "addresses can't be negative".into()))?;
        assemble(text, ip, bits).map(Value::bytes).map_err(|e| Fail::Arg(0, e))
    }

    /// find ROP gadgets: short runs of instructions ending in `ret`, the pieces a return-oriented exploit chains together when it can't run code of its own
    ///
    /// Scans the executable code of a binary (or raw x86/x64 bytes) for every clean run of up to `depth` instructions (5 by default) that ends in a `ret`, dropping any run a jump or call breaks partway through. Returns a table of addr, bytes and text. {jop: true} also finds jump-oriented gadgets, which end in a `jmp` or `call` through a register. {syntax: "att"}, {bits: 32} and {bits: 16} work as in disasm. Filter the table for the effect you need, like `binary("a.out").gadgets().filter(|g| g.text.contains("pop rdi"))`
    #[builtin(
        examples = [r#"gadgets(b"\x5f\xc3")"#],
        shown = [r#"binary("a.out").gadgets().text"#, r#"binary("a.out").gadgets({jop: true}).len"#],
        see = ["disasm", "binary", "strings"]
    )]
    fn gadgets(code: Or<&[u8], &Map>, opts: Option<&Map>) -> Result<As<Table>, Fail> {
        let rows = gadget_rows(code, opts)?
            .into_iter()
            .map(|(addr, bytes, text)| row([("addr", hex(addr)), ("bytes", Value::Bytes(bytes.into())), ("text", Value::str(text))]));
        Ok(table(rows)?.into())
    }

    /// find the instructions in a binary that call, jump to or read from an address or symbol
    ///
    /// A linear sweep of the executable code, like disasm(), for each `call`, `jmp`, conditional jump or `rip`-relative operand that lands on the target. A library function is found through its PLT stub, so `binary(path).xrefs("puts")` finds the calls to `puts`. Returns a table of addr, bytes and text. Bytes are a whole executable, as in `disasm(bytes, "main")`
    #[builtin(shown = [r#"binary("a.out").xrefs("puts")"#, r#"binary("a.out").xrefs(0x401136).addr"#], see = ["disasm", "binary"])]
    fn xrefs(code: Or<&[u8], &Map>, target: Or<i64, &str>) -> Result<As<Table>, Fail> {
        let rows = xref_rows(code, target)?
            .into_iter()
            .map(|(_, addr, bytes, text)| row([("addr", hex(addr)), ("bytes", Value::Bytes(bytes.into())), ("text", Value::str(text))]));
        Ok(table(rows)?.into())
    }

    /// the address a byte of a binary's file is loaded at, through its segment table
    #[builtin(shown = [r#"binary("a.out").va(0x1040)"#], see = ["offset", "binary"])]
    fn va(b: &Map, offset: i64) -> Result<Value, Fail> {
        let off = u64::try_from(offset).map_err(|_| Fail::Arg(1, "offsets can't be negative".into()))?;
        let s = segments(b)?.into_iter().find(|s| s.offset <= off && off < s.offset + s.file_size);
        s.map(|s| hex(s.addr + (off - s.offset))).ok_or_else(|| Fail::Arg(1, format!("no segment loads file offset {off:#x}")))
    }

    /// where in a binary's file the byte at an address comes from, through its segment table
    #[builtin(shown = [r#"binary("a.out").offset(0x401000)"#], see = ["va", "binary"])]
    fn offset(b: &Map, addr: i64) -> Result<Value, Fail> {
        let va = u64::try_from(addr).map_err(|_| Fail::Arg(1, "addresses can't be negative".into()))?;
        let s = segments(b)?.into_iter().find(|s| s.addr <= va && va < s.addr + s.size);
        let s = s.ok_or_else(|| Fail::Arg(1, format!("no segment covers address {va:#x}")))?;
        if va - s.addr >= s.file_size {
            return Err(Fail::Arg(1, format!("address {va:#x} is zero-filled at load time, so it isn't in the file")));
        }
        Ok(hex(s.offset + (va - s.addr)))
    }

    /// turn a compiler-mangled symbol name back into the Rust or C++ name it came from, like `c++filt`
    ///
    /// Names that aren't mangled come back unchanged
    #[builtin(examples = [r#"demangle("_ZN3foo3barEv")"#, r#"demangle("_ZN4core3fmt5write17h0123456789abcdefE")"#], see = ["binary"])]
    fn demangle(sym: &str) -> String {
        demangled(sym).unwrap_or_else(|| sym.to_string())
    }
}

/// A row of `binary`'s segments table.
struct Segment {
    addr: u64,
    size: u64,
    offset: u64,
    file_size: u64,
}

fn segments(b: &Map) -> Result<Vec<Segment>, Fail> {
    let Some(Value::Table(t)) = b.borrow().get("segments").cloned() else { return Err(Fail::Arg(0, "expected a map from binary()".into())) };
    let n = |m: &IndexMap<String, Value>, k: &str| match m.get(k) {
        Some(Value::Int(n, _)) => u64::try_from(*n).ok(),
        _ => None,
    };
    let rows = t.maps().into_iter().filter_map(|r| {
        let Value::Map(m) = r else { return None };
        let m = m.borrow();
        Some(Segment { addr: n(&m, "addr")?, size: n(&m, "size")?, offset: n(&m, "offset")?, file_size: n(&m, "file_size")? })
    });
    Ok(rows.collect())
}

/// A Rust (legacy or v0) or Itanium C++ name, demangled; Mach-O's extra leading `_` is allowed.
fn demangled(sym: &str) -> Option<String> {
    if let Ok(d) = rustc_demangle::try_demangle(sym) {
        return Some(format!("{d:#}"));
    }
    let cpp = sym.strip_prefix('_').filter(|s| s.starts_with("_Z")).unwrap_or(sym);
    cpp.starts_with("_Z").then(|| cpp_demangle::Symbol::new(cpp).ok()?.demangle().ok()).flatten()
}

/// An instruction's address, bytes and text, after the name of the function or section it starts, if any.
type Ins = (Option<String>, u64, Vec<u8>, String);
/// A named piece of code, its bytes and its address.
type Chunk<'a> = (Option<String>, &'a [u8], u64);

/// The instructions `disasm` is asked for; `objdump` writes operands as objdump does.
fn instructions(code: Or<&[u8], &Map>, at: Option<Or<i64, &str>>, opts: Option<&Map>, objdump: bool) -> Result<Vec<Ins>, Fail> {
    let (att, bits) = (att_opt(opts, 2)?, bits_opt(opts, 2)?);
    let at = match at {
        Some(Or::A(n)) => Some(Or::A(u64::try_from(n).map_err(|_| Fail::Arg(1, "addresses can't be negative".into()))?)),
        Some(Or::B(name)) => Some(Or::B(name)),
        None => None,
    };
    let exe = matches!(code, Or::B(_)) || matches!(at, Some(Or::B(_)));
    let data;
    let file = match code {
        Or::A(b) => b,
        Or::B(m) => {
            data = path_bytes(m, "disassemble them with disasm(bytes, \"main\")")?;
            &data[..]
        }
    };
    let (chunks, bits, to_ret) = match at {
        _ if exe => self::code(file, at)?,
        Some(Or::A(addr)) => (vec![(None, file, addr)], bits, false),
        _ => (vec![(None, file, 0)], bits, false),
    };
    Ok(decode(&chunks, bits, att, to_ret, objdump))
}

/// The code in an executable at a symbol or address, or all of it: its named pieces and their addresses, its bitness, and whether to stop at the first `ret` because the size isn't known.
fn code<'a>(data: &'a [u8], at: Option<Or<u64, &str>>) -> Result<(Vec<Chunk<'a>>, u32, bool), Fail> {
    let f = object::File::parse(data).map_err(|_| Fail::Arg(0, "not an ELF, PE or Mach-O file".into()))?;
    let bits = match f.architecture() {
        Architecture::X86_64 => 64,
        Architecture::I386 => 32,
        _ => return Err(Fail::Arg(0, "only x86/x64 are supported".into())),
    };
    let (what, addr, size) = match at {
        Some(Or::B(name)) => {
            let s = find_symbol(&f, name);
            let hint = if stripped(&f) { "; the file is stripped, so disassemble by address, like binary(path).disasm(addr)" } else { "" };
            let s = s.ok_or_else(|| Fail::Arg(1, format!("no symbol {name:?}{hint}")))?;
            (name.to_string(), s.address(), s.size())
        }
        Some(Or::A(addr)) => (format!("{addr:#x}"), addr, 0),
        None => {
            let code = |s: &object::Section| s.kind() == SectionKind::Text;
            let named = |s: object::Section<'a, '_>| Some((Some(s.name().ok()?.to_string()), s.data().ok()?, s.address()));
            let mut chunks: Vec<_> = f.sections().filter(code).filter_map(named).collect();
            // Files stripped down to no section headers still have their executable segments.
            if chunks.is_empty() {
                chunks = f.segments().filter(|s| s.permissions().executable()).filter_map(|s| Some((None, s.data().ok()?, s.address()))).collect();
            }
            return Ok((chunks, bits, false));
        }
    };
    let seg = f.segments().find(|s| s.address() <= addr && addr < s.address() + s.size()).ok_or_else(|| Fail::Arg(1, format!("{what} isn't in the file")))?;
    // Mach-O and COFF symbols and bare addresses have no size: take the rest of the segment and stop at the first `ret` or `hlt`.
    let len = if size > 0 { size } else { seg.address() + seg.size() - addr };
    let code = seg.data_range(addr, len).ok().flatten().ok_or_else(|| Fail::Arg(1, format!("{what} has no code in the file")))?;
    let label = matches!(at, Some(Or::B(_))).then_some(what);
    Ok((vec![(label, code, addr)], bits, size == 0))
}

pub(super) fn read_path(p: &str) -> Result<Vec<u8>, Fail> {
    crate::host::get().read(p).map_err(|e| Fail::Arg(0, format!("cannot read {p:?}: {e}")))
}

/// The file a `binary(path)` map came from, read again; `retry` says what to do instead for one read from bytes.
fn path_bytes(m: &Map, retry: &str) -> Result<Vec<u8>, Fail> {
    let Some(Value::Str(p)) = m.borrow().get("path").cloned() else {
        return Err(Fail::Arg(0, format!("this binary was read from bytes; {retry}")));
    };
    read_path(&p)
}

/// `{bits: 16 | 32 | 64}` from the options, argument `arg`; 64 by default.
fn bits_opt(opts: Option<&Map>, arg: usize) -> Result<u32, Fail> {
    match opts.and_then(|m| m.borrow().get("bits").cloned()) {
        None => Ok(64),
        Some(Value::Int(n @ (16 | 32 | 64), _)) => Ok(n as u32),
        Some(_) => Err(Fail::Arg(arg, "bits is 16, 32 or 64".into())),
    }
}

/// Whether the options, argument `arg`, ask for `{syntax: "att"}` over the default "intel".
fn att_opt(opts: Option<&Map>, arg: usize) -> Result<bool, Fail> {
    match opts.and_then(|m| m.borrow().get("syntax").cloned()) {
        None => Ok(false),
        Some(Value::Str(s)) if &*s == "intel" || &*s == "att" => Ok(&*s == "att"),
        Some(_) => Err(Fail::Arg(arg, "syntax is \"intel\" or \"att\"".into())),
    }
}

/// The symbol defining `name`, or Mach-O's `_name`, its C name.
fn find_symbol<'d, 'f>(f: &'f object::File<'d>, name: &str) -> Option<object::Symbol<'d, 'f>> {
    let alt = format!("_{name}");
    f.symbols().chain(f.dynamic_symbols()).find(|s| s.is_definition() && s.name().is_ok_and(|n| n == name || n == alt))
}

/// No symbol table beyond what dynamic linking needs.
fn stripped(f: &object::File) -> bool {
    f.symbols().next().is_none()
}

// ponytail: stopping at the first `ret` cuts functions with early returns short; bound by the next symbol if that matters.
fn decode(chunks: &[Chunk], bits: u32, att: bool, to_ret: bool, objdump: bool) -> Vec<Ins> {
    let (mut intel, mut gas) = (IntelFormatter::new(), GasFormatter::new());
    let fmt: &mut dyn Formatter = if att { &mut gas } else { &mut intel };
    if objdump {
        let o = fmt.options_mut();
        o.set_hex_prefix("0x");
        o.set_hex_suffix("");
        o.set_uppercase_hex(false);
        o.set_small_hex_numbers_in_decimal(false);
        o.set_branch_leading_zeros(false);
        o.set_first_operand_char_index(7);
        o.set_memory_size_options(MemorySizeOptions::Always);
    }
    let mut out = vec![];
    for (label, code, addr) in chunks {
        let mut label = label.clone();
        for ins in Decoder::with_ip(bits, code, *addr, DecoderOptions::NONE) {
            let mut text = String::new();
            fmt.format(&ins, &mut text);
            let at = (ins.ip() - addr) as usize;
            out.push((label.take(), ins.ip(), code[at..at + ins.len()].to_vec(), text));
            if to_ret && matches!(ins.mnemonic(), Mnemonic::Ret | Mnemonic::Hlt) {
                break;
            }
        }
    }
    out
}

/// The gadgets `gadgets` is asked for, each an address, its bytes and its instructions joined with `; `.
fn gadget_rows(code: Or<&[u8], &Map>, opts: Option<&Map>) -> Result<Vec<(u64, Vec<u8>, String)>, Fail> {
    let opt = |k: &str| opts.and_then(|m| m.borrow().get(k).cloned());
    let (att, mut bits) = (att_opt(opts, 1)?, bits_opt(opts, 1)?);
    let depth = match opt("depth") {
        None => 5,
        Some(Value::Int(n @ 1..=64, _)) => n as usize,
        Some(_) => return Err(Fail::Arg(1, "depth is 1 to 64".into())),
    };
    let jop = match opt("jop") {
        None => false,
        Some(Value::Bool(b)) => b,
        Some(_) => return Err(Fail::Arg(1, "jop is true or false".into())),
    };
    // Raw bytes are scanned as-is from address 0; a binary's executable sections are scanned at their load addresses.
    let data;
    let chunks: Vec<(&[u8], u64)> = match code {
        Or::A(b) => vec![(b, 0)],
        Or::B(m) => {
            data = path_bytes(m, "scan them with gadgets(bytes)")?;
            let (cs, b, _) = self::code(&data, None)?;
            bits = b;
            cs.into_iter().map(|(_, bytes, addr)| (bytes, addr)).collect()
        }
    };
    let (mut intel, mut gas) = (IntelFormatter::new(), GasFormatter::new());
    let fmt: &mut dyn Formatter = if att { &mut gas } else { &mut intel };
    let mut out = vec![];
    for (bytes, base) in &chunks {
        find_gadgets(bytes, *base, bits, depth, jop, fmt, &mut out);
    }
    Ok(out)
}

// ponytail: decodes forward from every byte offset (O(bytes·depth)); fine for real binaries, a suffix-based pass is the upgrade if it ever drags.
fn find_gadgets(code: &[u8], base: u64, bits: u32, depth: usize, jop: bool, fmt: &mut dyn Formatter, out: &mut Vec<(u64, Vec<u8>, String)>) {
    let mut decoder = Decoder::new(bits, code, DecoderOptions::NONE);
    for start in 0..code.len() {
        if decoder.set_position(start).is_err() {
            continue;
        }
        decoder.set_ip(base + start as u64);
        let mut texts: Vec<String> = vec![];
        // Walk instructions until one ends the gadget (a `ret`, or an indirect `jmp`/`call` for jop), a branch breaks it, or it runs too long.
        while texts.len() < depth && decoder.can_decode() {
            let ins = decoder.decode();
            if ins.is_invalid() {
                break;
            }
            let mut text = String::new();
            fmt.format(&ins, &mut text);
            let ends = match ins.flow_control() {
                FlowControl::Next => {
                    texts.push(text);
                    continue;
                }
                FlowControl::Return => true,
                FlowControl::IndirectBranch | FlowControl::IndirectCall => jop,
                _ => false,
            };
            if ends {
                texts.push(text);
                out.push((base + start as u64, code[start..decoder.position()].to_vec(), texts.join("; ")));
            }
            break;
        }
    }
}

/// The instructions in an executable's code that branch to or address `target`, a symbol, PLT stub or address.
fn xref_rows(code: Or<&[u8], &Map>, target: Or<i64, &str>) -> Result<Vec<Ins>, Fail> {
    let data;
    let file = match code {
        Or::A(b) => b,
        Or::B(m) => {
            data = path_bytes(m, "search them with xrefs(bytes, target)")?;
            &data[..]
        }
    };
    let to = match target {
        Or::A(n) => u64::try_from(n).map_err(|_| Fail::Arg(1, "addresses can't be negative".into()))?,
        Or::B(name) => {
            let f = object::File::parse(file).map_err(|_| Fail::Arg(0, "not an ELF, PE or Mach-O file".into()))?;
            let sym = find_symbol(&f, name);
            let plt = || match elf_got_plt(&f, &relocations(&f)).1 {
                Value::Map(m) => m.borrow().get(name).cloned(),
                _ => None,
            };
            match (sym, plt()) {
                (Some(s), _) => s.address(),
                (None, Some(Value::Int(n, _))) => n as u64,
                _ => return Err(Fail::Arg(1, format!("no symbol or PLT stub {name:?}"))),
            }
        }
    };
    let (chunks, bits, _) = self::code(file, None)?;
    let hits = |i: &iced_x86::Instruction| {
        (0..i.op_count()).any(|k| matches!(i.op_kind(k), OpKind::NearBranch16 | OpKind::NearBranch32 | OpKind::NearBranch64)) && i.near_branch_target() == to
            || i.is_ip_rel_memory_operand() && i.ip_rel_memory_address() == to
    };
    let mut fmt = IntelFormatter::new();
    let mut out = vec![];
    for (_, bytes, addr) in &chunks {
        for ins in Decoder::with_ip(bits, bytes, *addr, DecoderOptions::NONE).into_iter().filter(hits) {
            let mut text = String::new();
            fmt.format(&ins, &mut text);
            let at = (ins.ip() - addr) as usize;
            out.push((None, ins.ip(), bytes[at..at + ins.len()].to_vec(), text));
        }
    }
    Ok(out)
}

#[derive(Clone, Copy)]
enum Operand {
    Reg(Register),
    Imm(i64),
    Mem { size: Option<usize>, seg: Register, base: Register, index: Register, scale: u32, disp: i64 },
}

/// Intel-syntax instructions separated by `;` or new lines, each encoded in its shortest form.
fn assemble(src: &str, ip: u64, bits: u32) -> Result<Vec<u8>, String> {
    let regs: HashMap<String, Register> = Register::values().map(|r| (format!("{r:?}").to_lowercase(), r)).collect();
    let mut out = vec![];
    for line in src.split([';', '\n']).map(str::trim).filter(|l| !l.is_empty()) {
        out.extend(assemble_one(line, ip + out.len() as u64, bits, &regs).map_err(|e| format!("{line:?}: {e}"))?);
    }
    Ok(out)
}

fn assemble_one(line: &str, ip: u64, bits: u32, regs: &HashMap<String, Register>) -> Result<Vec<u8>, String> {
    let (mut rest, mut prefixes) = (line, vec![]);
    let name = loop {
        let (w, r) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
        rest = r.trim();
        match w.to_lowercase().as_str() {
            p @ ("lock" | "rep" | "repe" | "repz" | "repne" | "repnz") => prefixes.push(p.to_string()),
            "jz" => break "je".to_string(),
            "jnz" => break "jne".to_string(),
            w => break w.to_string(),
        }
    };
    let ops = if rest.is_empty() { vec![] } else { rest.split(',').map(|o| operand(o, regs)).collect::<Result<Vec<_>, _>>()? };
    let codes: Vec<Code> = Code::values().filter(|c| format!("{:?}", c.mnemonic()).eq_ignore_ascii_case(&name)).collect();
    if codes.is_empty() {
        return Err(format!("unknown instruction {name:?}"));
    }
    let encode = |c: Code| {
        // A `rip`-relative target depends on the instruction's length, so encode twice.
        let len = Encoder::new(bits).encode(&instruction(c, &ops, ip, bits, &prefixes)?, ip).ok()?;
        let ins = instruction(c, &ops, ip + len as u64, bits, &prefixes)?;
        let mut enc = Encoder::new(bits);
        enc.encode(&ins, ip).ok()?;
        Some((enc.take_buffer(), ins.memory_size().size()))
    };
    let found: Vec<_> = codes.into_iter().filter_map(encode).collect();
    let unsized_mem = ops.iter().any(|o| matches!(o, Operand::Mem { size: None, .. }));
    if unsized_mem && found.iter().any(|f| f.1 != found[0].1) {
        return Err("the memory operand's size is ambiguous; write byte, word, dword or qword ptr".into());
    }
    found.into_iter().map(|f| f.0).min_by_key(Vec::len).ok_or_else(|| format!("{name} doesn't take these operands in {bits}-bit code"))
}

/// An `Instruction` of `code` with these operands, or `None` if they can't fit it.
fn instruction(code: Code, ops: &[Operand], next: u64, bits: u32, prefixes: &[String]) -> Option<Instruction> {
    use iced_x86::OpCodeOperandKind as K;
    let oc = code.op_code();
    let mut ins = Instruction::default();
    ins.set_code(code);
    let mode = match bits {
        64 => oc.mode64(),
        32 => oc.mode32(),
        _ => oc.mode16(),
    };
    if !mode {
        return None;
    }
    if ops.is_empty() && ins.op_count() > 0 {
        string_operands(&mut ins, bits)?;
    } else if ins.op_count() as usize != ops.len() {
        return None;
    }
    for (i, op) in ops.iter().enumerate() {
        let i = i as u32;
        match *op {
            Operand::Reg(r) => {
                ins.set_op_kind(i, OpKind::Register);
                ins.set_op_register(i, r);
            }
            Operand::Mem { size, seg, base, index, scale, disp } => {
                if size.is_some_and(|n| n != ins.memory_size().size()) {
                    return None;
                }
                let rip = matches!(base, Register::RIP | Register::EIP);
                ins.set_op_kind(i, OpKind::Memory);
                ins.set_segment_prefix(seg);
                ins.set_memory_base(base);
                ins.set_memory_index(index);
                ins.set_memory_index_scale(scale);
                ins.set_memory_displacement64(if rip { next.wrapping_add(disp as u64) } else { disp as u64 });
                ins.set_memory_displ_size(u32::from(disp != 0));
            }
            Operand::Imm(v) => {
                let i8s = (-128..=127).contains(&v);
                let (kind, fits) = match oc.op_kind(i) {
                    K::imm8 => (OpKind::Immediate8, (-128..=255).contains(&v)),
                    K::imm8_const_1 => (OpKind::Immediate8, v == 1),
                    K::imm8sex16 => (OpKind::Immediate8to16, i8s),
                    K::imm8sex32 => (OpKind::Immediate8to32, i8s),
                    K::imm8sex64 => (OpKind::Immediate8to64, i8s),
                    K::imm16 => (OpKind::Immediate16, (-0x8000..=0xffff).contains(&v)),
                    K::imm32 => (OpKind::Immediate32, (i32::MIN as i64..=u32::MAX as i64).contains(&v)),
                    K::imm32sex64 => (OpKind::Immediate32to64, i32::try_from(v).is_ok()),
                    K::imm64 => (OpKind::Immediate64, true),
                    // An operand-size prefix would truncate the target, so a branch's width is the code's.
                    K::br16_1 | K::br16_2 => (OpKind::NearBranch16, bits == 16),
                    K::br32_1 | K::br32_4 => (OpKind::NearBranch32, bits == 32),
                    K::br64_1 | K::br64_4 => (OpKind::NearBranch64, bits == 64),
                    _ => return None,
                };
                if !fits {
                    return None;
                }
                ins.set_op_kind(i, kind);
                match kind {
                    OpKind::NearBranch16 => ins.set_near_branch16(v as u16),
                    OpKind::NearBranch32 => ins.set_near_branch32(v as u32),
                    OpKind::NearBranch64 => ins.set_near_branch64(v as u64),
                    _ => ins.try_set_immediate_i64(i, v).ok()?,
                }
            }
        }
    }
    for p in prefixes {
        match p.as_str() {
            "lock" => ins.set_has_lock_prefix(true),
            "rep" | "repe" | "repz" => ins.set_has_repe_prefix(true),
            _ => ins.set_has_repne_prefix(true),
        }
    }
    Some(ins)
}

/// The implicit operands of a string instruction written bare, like `movsb` or `stosq`.
fn string_operands(ins: &mut Instruction, bits: u32) -> Option<()> {
    use iced_x86::OpCodeOperandKind as K;
    let (si, di) = match bits {
        64 => (OpKind::MemorySegRSI, OpKind::MemoryESRDI),
        32 => (OpKind::MemorySegESI, OpKind::MemoryESEDI),
        _ => (OpKind::MemorySegSI, OpKind::MemoryESDI),
    };
    for i in 0..ins.op_count() {
        let reg = match ins.op_code().op_kind(i) {
            K::seg_rSI => Err(si),
            K::es_rDI => Err(di),
            K::al => Ok(Register::AL),
            K::ax => Ok(Register::AX),
            K::eax => Ok(Register::EAX),
            K::rax => Ok(Register::RAX),
            K::dx => Ok(Register::DX),
            _ => return None,
        };
        match reg {
            Ok(r) => {
                ins.set_op_kind(i, OpKind::Register);
                ins.set_op_register(i, r);
            }
            Err(kind) => ins.set_op_kind(i, kind),
        }
    }
    Some(())
}

/// A register, an integer, or memory like `qword ptr fs:[rax + rbx*8 - 0x10]`.
fn operand(s: &str, regs: &HashMap<String, Register>) -> Result<Operand, String> {
    let s = s.trim().to_lowercase();
    let bad = || format!("can't read operand {s:?}");
    let int = |t: &str| {
        let (neg, t) = t.strip_prefix('-').map_or((false, t), |t| (true, t.trim_start()));
        let n = match t.strip_prefix("0x") {
            Some(h) => u64::from_str_radix(h, 16).ok(),
            None => t.parse::<u64>().ok(),
        };
        n.map(|n| if neg { (n as i64).wrapping_neg() } else { n as i64 })
    };
    let sizes = [("byte", 1), ("word", 2), ("dword", 4), ("qword", 8), ("tword", 10), ("xmmword", 16), ("ymmword", 32), ("zmmword", 64)];
    let (w, after) = s.split_once(char::is_whitespace).unwrap_or((&s, ""));
    let (size, rest) = match sizes.iter().find(|(k, _)| *k == w) {
        Some(&(_, n)) => (Some(n), after.trim().strip_prefix("ptr").unwrap_or(after).trim()),
        None => (None, s.as_str()),
    };
    let Some((seg, mem)) = rest.split_once('[') else {
        return regs.get(rest).map(|&r| Operand::Reg(r)).or_else(|| int(rest).map(Operand::Imm)).ok_or_else(bad);
    };
    let seg = seg.trim().trim_end_matches(':').trim();
    let seg = if seg.is_empty() { Register::None } else { *regs.get(seg).ok_or_else(bad)? };
    let (mut base, mut index, mut scale, mut disp) = (Register::None, Register::None, 1, 0i64);
    for t in mem.strip_suffix(']').ok_or_else(bad)?.replace('-', "+-").split('+').map(str::trim).filter(|t| !t.is_empty()) {
        if let Some((a, b)) = t.split_once('*') {
            let (r, n) = if regs.contains_key(a.trim()) { (a.trim(), b.trim()) } else { (b.trim(), a.trim()) };
            index = *regs.get(r).ok_or_else(bad)?;
            scale = int(n).filter(|n| matches!(n, 1 | 2 | 4 | 8)).ok_or_else(|| format!("the scale in {s:?} must be 1, 2, 4 or 8"))? as u32;
        } else if let Some(&r) = regs.get(t) {
            if base == Register::None {
                base = r;
            } else {
                index = r;
            }
        } else {
            disp = disp.wrapping_add(int(t).ok_or_else(bad)?);
        }
    }
    Ok(Operand::Mem { size, seg, base, index, scale, disp })
}

/// Instructions as `objdump -d` lays them out.
fn listing(ins: &[Ins]) -> String {
    let w = ins.iter().map(|i| format!("{:x}", i.1).len()).max().unwrap_or(0);
    let mut lines = vec![];
    for (label, addr, bytes, text) in ins {
        if let Some(l) = label {
            if !lines.is_empty() {
                lines.push(String::new());
            }
            lines.push(format!("{addr:0w$x} <{l}>:"));
        }
        let hex: Vec<_> = bytes.iter().map(|b| format!("{b:02x}")).collect();
        lines.push(format!("{addr:>w$x}:  {:<21}  {text}", hex.join(" ")));
    }
    lines.join("\n")
}

fn binary(data: &[u8], path: Option<&str>) -> Call {
    let f = object::File::parse(data).map_err(|_| Fail::Arg(0, "not an ELF, PE or Mach-O file".into()))?;
    let format = match f {
        object::File::Elf32(_) | object::File::Elf64(_) => "elf",
        object::File::MachO32(_) | object::File::MachO64(_) => "macho",
        _ => "pe",
    };
    let opt = |s: Option<&str>| s.map_or(Value::Nil, Value::str);
    let sections = f.sections().map(|s| {
        row([
            ("name", Value::str(s.name().unwrap_or(""))),
            ("addr", hex(s.address())),
            ("size", int(s.size())),
            ("offset", s.file_range().map_or(Value::Nil, |r| hex(r.0))),
        ])
    });
    let segments = f.segments().map(|s| {
        let p = s.permissions();
        let perm: String = [(p.readable(), 'r'), (p.writable(), 'w'), (p.executable(), 'x')].iter().map(|&(on, c)| if on { c } else { '-' }).collect();
        row([
            ("name", opt(s.name().ok().flatten())),
            ("addr", hex(s.address())),
            ("size", int(s.size())),
            ("offset", hex(s.file_range().0)),
            ("file_size", int(s.file_range().1)),
            ("perm", Value::str(perm)),
        ])
    });
    // A stripped file keeps only its dynamic symbols.
    let symbols = if stripped(&f) { f.dynamic_symbols() } else { f.symbols() };
    let symbols = symbols.filter(|s| s.is_definition() && !s.name().unwrap_or("").is_empty()).map(|s| {
        let kind = format!("{:?}", s.kind()).to_lowercase();
        row([
            ("name", Value::str(s.name().unwrap_or(""))),
            ("addr", hex(s.address())),
            ("size", int(s.size())),
            ("kind", Value::str(kind)),
            ("global", Value::Bool(s.is_global())),
            ("demangled", s.name().ok().and_then(demangled).map_or(Value::Nil, Value::str)),
        ])
    });
    let text = |b: &[u8]| Value::str(String::from_utf8_lossy(b));
    // Unreadable import and export tables come out empty.
    let imports: Vec<_> = f.imports().into_iter().flatten().flatten().collect();
    let exports = f.exports().into_iter().flatten().flatten().map(|e| {
        let addr = match e.target() {
            object::ExportTarget::Address { address: n } | object::ExportTarget::Absolute { value: n } => hex(n),
            _ => Value::Nil,
        };
        row([("name", name(e.name())), ("addr", addr)])
    });
    // Stack protectors call or read these; stripped PE files keep no trace of `/GS`.
    // ponytail: name heuristic; read the PE load config's security cookie if PE canaries matter.
    let names = f.symbols().chain(f.dynamic_symbols()).filter_map(|s| s.name().ok().map(str::to_string));
    let names: Vec<String> = names.chain(imports.iter().map(|i| name(i.name()).to_string())).collect();
    let canary =
        names.iter().any(|n| matches!(n.trim_start_matches('_'), "stack_chk_fail" | "stack_chk_guard" | "security_check_cookie" | "security_init_cookie"));
    let relocs = relocations(&f);
    let (got, plt) = if format == "elf" { elf_got_plt(&f, &relocs) } else { (Value::Nil, Value::Nil) };
    let relocs = relocs
        .into_iter()
        .map(|r| row([("offset", hex(r.offset)), ("kind", Value::str(r.kind)), ("symbol", opt(r.symbol.as_deref())), ("addend", Value::int(r.addend))]));
    let (pie, nx, relro) = match &f {
        object::File::Elf32(e) => elf_hardening(e),
        object::File::Elf64(e) => elf_hardening(e),
        object::File::MachO32(m) => macho_hardening(m.macho_header().flags(m.endian())),
        object::File::MachO64(m) => macho_hardening(m.macho_header().flags(m.endian())),
        object::File::Pe32(p) => pe_hardening(p.nt_headers().optional_header().dll_characteristics()),
        object::File::Pe64(p) => pe_hardening(p.nt_headers().optional_header().dll_characteristics()),
        _ => (false, false, Value::Nil),
    };
    let fields = [
        ("path", opt(path)),
        ("format", Value::str(format)),
        ("arch", Value::str(format!("{:?}", f.architecture()).to_lowercase())),
        ("bits", Value::int(if f.is_64() { 64 } else { 32 })),
        ("endian", Value::str(if f.is_little_endian() { "little" } else { "big" })),
        ("entry", hex(f.entry())),
        ("sections", table(sections)?),
        ("segments", table(segments)?),
        ("symbols", table(symbols)?),
        ("stripped", Value::Bool(stripped(&f))),
        ("imports", table(imports.iter().map(|i| row([("name", name(i.name())), ("library", text(i.library()))])))?),
        ("exports", table(exports)?),
        ("relocs", table(relocs)?),
        ("got", got),
        ("plt", plt),
        ("pie", Value::Bool(pie)),
        ("nx", Value::Bool(nx)),
        ("canary", Value::Bool(canary)),
        ("relro", relro),
    ];
    Ok(Value::map(fields.into_iter().map(|(k, v)| (k.to_string(), v)).collect()))
}

struct Reloc {
    offset: u64,
    kind: String,
    symbol: Option<String>,
    addend: i64,
}

/// The dynamic linker's relocations, then each section's (for object files, at the section's address plus the offset).
fn relocations(f: &object::File) -> Vec<Reloc> {
    let dynsyms = f.dynamic_symbol_table();
    let dynamic = f.dynamic_relocations().into_iter().flatten().map(|(at, r)| {
        let sym = match r.target() {
            RelocationTarget::Symbol(i) => dynsyms.as_ref().and_then(|t| t.symbol_by_index(i).ok()).and_then(|s| s.name().ok().map(str::to_string)),
            _ => None,
        };
        (at, r, sym)
    });
    let in_sections = f.sections().flat_map(|s| {
        s.relocations().map(move |(off, r)| {
            // A reloc against a section symbol names that section.
            let sym = match r.target() {
                RelocationTarget::Symbol(i) => f.symbol_by_index(i).ok().and_then(|s| match s.name() {
                    Ok("") | Err(_) => s.section_index().and_then(|i| f.section_by_index(i).ok()?.name().ok().map(str::to_string)),
                    Ok(n) => Some(n.to_string()),
                }),
                RelocationTarget::Section(i) => f.section_by_index(i).ok().and_then(|s| s.name().ok().map(str::to_string)),
                _ => None,
            };
            (s.address() + off, r, sym)
        })
    });
    let arch = f.architecture();
    dynamic.chain(in_sections).map(|(offset, r, symbol)| Reloc { offset, kind: reloc_kind(arch, &r), symbol, addend: r.addend() }).collect()
}

/// The ELF name of an x86 or x64 relocation, like "jump_slot" for `R_X86_64_JUMP_SLOT`, else object's generic kind.
fn reloc_kind(arch: Architecture, r: &object::Relocation) -> String {
    let name = match (arch, r.flags()) {
        (Architecture::X86_64, RelocationFlags::Elf { r_type }) => match r_type {
            elf::R_X86_64_64 => "64",
            elf::R_X86_64_PC32 => "pc32",
            elf::R_X86_64_GOT32 => "got32",
            elf::R_X86_64_PLT32 => "plt32",
            elf::R_X86_64_COPY => "copy",
            elf::R_X86_64_GLOB_DAT => "glob_dat",
            elf::R_X86_64_JUMP_SLOT => "jump_slot",
            elf::R_X86_64_RELATIVE => "relative",
            elf::R_X86_64_GOTPCREL => "gotpcrel",
            elf::R_X86_64_32 => "32",
            elf::R_X86_64_32S => "32s",
            elf::R_X86_64_DTPMOD64 => "dtpmod64",
            elf::R_X86_64_DTPOFF64 => "dtpoff64",
            elf::R_X86_64_TPOFF64 => "tpoff64",
            elf::R_X86_64_IRELATIVE => "irelative",
            elf::R_X86_64_GOTPCRELX => "gotpcrelx",
            elf::R_X86_64_REX_GOTPCRELX => "rex_gotpcrelx",
            _ => "",
        },
        (Architecture::I386, RelocationFlags::Elf { r_type }) => match r_type {
            elf::R_386_32 => "32",
            elf::R_386_PC32 => "pc32",
            elf::R_386_GOT32 => "got32",
            elf::R_386_PLT32 => "plt32",
            elf::R_386_COPY => "copy",
            elf::R_386_GLOB_DAT => "glob_dat",
            elf::R_386_JMP_SLOT => "jump_slot",
            elf::R_386_RELATIVE => "relative",
            elf::R_386_GOTOFF => "gotoff",
            elf::R_386_GOTPC => "gotpc",
            elf::R_386_IRELATIVE => "irelative",
            elf::R_386_GOT32X => "got32x",
            _ => "",
        },
        _ => "",
    };
    if name.is_empty() { format!("{:?}", r.kind()).to_lowercase() } else { name.to_string() }
}

/// Each imported symbol's GOT slot (from its `glob_dat` or `jump_slot` reloc) and PLT stub, as maps from name to address.
// ponytail: PLT stubs are found by layout (16 bytes each, in `.rela.plt` order, after `.plt`'s header or from the start of `.plt.sec`), which holds for x86 GCC and Clang output; decode the stubs if other layouts matter.
fn elf_got_plt(f: &object::File, relocs: &[Reloc]) -> (Value, Value) {
    let named =
        |kinds: &'static [&'static str]| relocs.iter().filter(move |r| kinds.contains(&r.kind.as_str())).filter_map(|r| Some((r.symbol.clone()?, r.offset)));
    let got = named(&["glob_dat", "jump_slot"]).map(|(n, a)| (n, hex(a))).collect();
    let addr = |name: &str| f.section_by_name(name).map(|s| s.address());
    let stubs = match (addr(".plt.sec"), addr(".plt")) {
        (Some(sec), _) => Some(sec),
        (None, Some(plt)) => Some(plt + 16),
        _ => None,
    };
    let plt = stubs.map(|base| named(&["jump_slot"]).enumerate().map(|(i, (n, _))| (n, hex(base + 16 * i as u64))).collect()).unwrap_or_default();
    (Value::map(got), Value::map(plt))
}

/// PIE is a dynamic ELF, NX a non-executable `PT_GNU_STACK`, and RELRO "partial" with `PT_GNU_RELRO` and "full" when it also binds now.
fn elf_hardening<H: FileHeader<Endian = Endianness>>(f: &ElfFile<'_, H>) -> (bool, bool, Value) {
    let e = f.endian();
    let phdrs = f.elf_program_headers();
    let nx = phdrs.iter().find(|p| p.p_type(e) == elf::PT_GNU_STACK).is_some_and(|p| p.p_flags(e).0 & elf::PF_X.0 == 0);
    let dynamics = f.elf_section_table().dynamic(e, f.data()).ok().flatten().map_or(&[][..], |(d, _)| d);
    let now = dynamics.iter().any(|d| {
        let (tag, val): (_, u64) = (d.d_tag(e), d.d_val(e).into());
        tag == elf::DT_BIND_NOW || tag == elf::DT_FLAGS && val & elf::DF_BIND_NOW.0 != 0 || tag == elf::DT_FLAGS_1 && val & elf::DF_1_NOW.0 != 0
    });
    let relro = match (phdrs.iter().any(|p| p.p_type(e) == elf::PT_GNU_RELRO), now) {
        (false, _) => "none",
        (true, false) => "partial",
        (true, true) => "full",
    };
    (f.kind() == ObjectKind::Dynamic, nx, Value::str(relro))
}

/// Mach-O has no RELRO.
fn macho_hardening(flags: macho::FileFlags) -> (bool, bool, Value) {
    (flags.0 & macho::MH_PIE.0 != 0, flags.0 & macho::MH_ALLOW_STACK_EXECUTION.0 == 0, Value::Nil)
}

/// PIE is ASLR (`DYNAMIC_BASE`), NX is DEP (`NX_COMPAT`); PE has no RELRO.
fn pe_hardening(flags: pe::DllFlags) -> (bool, bool, Value) {
    (flags.0 & pe::IMAGE_DLLCHARACTERISTICS_DYNAMIC_BASE.0 != 0, flags.0 & pe::IMAGE_DLLCHARACTERISTICS_NX_COMPAT.0 != 0, Value::Nil)
}

/// A PE import or export can go by number instead of name.
fn name(n: object::NameOrOrdinal<&[u8]>) -> Value {
    match n {
        object::NameOrOrdinal::Name(b) => Value::str(String::from_utf8_lossy(b)),
        object::NameOrOrdinal::Ordinal(n) => Value::int(n.into()),
    }
}

fn row<const N: usize>(cells: [(&str, Value); N]) -> Value {
    Value::map(cells.into_iter().map(|(k, v)| (k.to_string(), v)).collect::<IndexMap<_, _>>())
}

fn table(rows: impl Iterator<Item = Value>) -> Result<Value, Fail> {
    Ok(Value::table(Table::from_maps(&rows.collect::<Vec<_>>()).map_err(|e| Fail::Arg(0, e))?))
}

fn int(n: u64) -> Value {
    i64::try_from(n).map_or_else(|_| Value::Big(std::rc::Rc::new(n.into()), Radix::DEC), Value::int)
}

/// Addresses show in hex.
fn hex(n: u64) -> Value {
    match int(n) {
        Value::Int(n, _) => Value::Int(n, Radix::HEX),
        Value::Big(n, _) => Value::Big(n, Radix::HEX),
        v => v,
    }
}

#[cfg(test)]
mod tests {
    use crate::interp::tests::{eval, show, try_eval};

    /// A 64-bit little-endian x86-64 PIE with a non-executable stack, a read-only load segment and no sections.
    fn elf() -> String {
        elf_with(&[])
    }

    /// The same, with code after the headers, in the load segment from 0x10b0.
    fn elf_with(code: &[u8]) -> String {
        let mut b = vec![0x7f, b'E', b'L', b'F', 2, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        let put = |b: &mut Vec<u8>, n: u64, w: usize| b.extend(&n.to_le_bytes()[..w]);
        // e_type DYN, e_machine x86-64, e_version, e_entry, e_phoff, e_shoff, e_flags, e_ehsize, e_phentsize, e_phnum, e_shentsize, e_shnum, e_shstrndx
        for (n, w) in [(3, 2), (62, 2), (1, 4), (0x1040, 8), (64, 8), (0, 8), (0, 4), (64, 2), (56, 2), (2, 2), (64, 2), (0, 2), (0, 2)] {
            put(&mut b, n, w);
        }
        // PT_LOAD r-x over the whole file at 0x1000 with 0x150 zero-filled bytes after it, then PT_GNU_STACK rw-.
        for (ty, flags, addr, size, mem) in [(1, 5, 0x1000, 176 + code.len() as u64, 0x200), (0x6474_e551, 6, 0, 0, 0)] {
            for (n, w) in [(ty, 4), (flags, 4), (0, 8), (addr, 8), (addr, 8), (size, 8), (mem, 8), (0x1000, 8)] {
                put(&mut b, n, w);
            }
        }
        b.extend(code);
        format!("binary({:?}.bytes)", b)
    }

    #[test]
    fn binary() {
        let b = elf();
        assert_eq!(show(&format!("x = {b}; [x.format, x.arch, x.bits, x.endian, x.entry]")), r#"["elf", "x86_64", 64, "little", 0x1040]"#);
        assert_eq!(show(&format!("x = {b}; [x.pie, x.nx, x.canary, x.relro]")), r#"[true, true, false, "none"]"#);
        assert_eq!(show(&format!("{b}.segments.perm")), r#"["r-x"]"#);
        assert_eq!(show(&format!("{b}.symbols.len")), "0");
        assert_eq!(show(&format!("{b}.stripped")), "true");
        assert_eq!(show(&format!("x = {b}; [x.relocs.len, x.got, x.plt]")), "[0, {}, {}]");
        assert!(try_eval(r#"binary(b"not an executable")"#).unwrap_err().msg.contains("not an ELF, PE or Mach-O file"));
    }

    #[test]
    fn addresses() {
        let b = elf();
        assert_eq!(show(&format!("x = {b}; [x.va(0x40), x.offset(0x1040), x.offset(x.va(0xaf))]")), "[0x1040, 0x40, 0xaf]");
        assert!(try_eval(&format!("{b}.va(176)")).unwrap_err().msg.contains("no segment loads file offset 0xb0"));
        assert!(try_eval(&format!("{b}.offset(0x10b0)")).unwrap_err().msg.contains("zero-filled"));
        assert!(try_eval(&format!("{b}.offset(0x1200)")).unwrap_err().msg.contains("no segment covers address 0x1200"));
    }

    #[test]
    fn xrefs() {
        // Nops to resync the sweep after the headers, then `call 0x10c4` at 0x10bf and `lea rax, [rip - 7]` (0x10c4) at 0x10c4.
        let code = [&[0x90; 15][..], &[0xe8, 0, 0, 0, 0], &[0x48, 0x8d, 0x05, 0xf9, 0xff, 0xff, 0xff]].concat();
        let b = elf_with(&code);
        let bytes = &b[7..b.len() - 1];
        assert_eq!(show(&format!("xrefs({bytes}, 0x10c4).addr")), "[0x10bf, 0x10c4]");
        assert_eq!(show(&format!("xrefs({bytes}, 0x10c5).len")), "0");
        assert!(try_eval(&format!(r#"xrefs({bytes}, "main")"#)).unwrap_err().msg.contains(r#"no symbol or PLT stub "main""#));
        assert!(try_eval(&format!("{b}.xrefs(0x10c4)")).unwrap_err().msg.contains("read from bytes"));
    }

    #[test]
    fn demangle() {
        assert_eq!(show(r#"demangle("_ZN3foo3barEv")"#), "foo::bar()");
        assert_eq!(show(r#"demangle("__ZN3foo3barEv")"#), "foo::bar()");
        assert_eq!(show(r#"demangle("_ZN4core3fmt5write17h0123456789abcdefE")"#), "core::fmt::write");
        assert_eq!(show(r#"demangle("_RNvCs1234_7mycrate3foo")"#), "mycrate::foo");
        assert_eq!(show(r#"demangle("main")"#), "main");
    }

    #[test]
    fn disasm() {
        let code = r#"b"\x55\x48\x89\xe5\xc3""#;
        assert_eq!(show(&format!("disasm({code}, nil, {{table: true}}).text")), r#"["push rbp", "mov rbp,rsp", "ret"]"#);
        assert_eq!(show(&format!(r#"disasm({code}, 0, {{syntax: "att", table: true}}).text"#)), r#"["push %rbp", "mov %rsp,%rbp", "ret"]"#);
        assert_eq!(show(&format!("disasm({code}, 0x401000, {{table: true}}).addr")), "[0x401000, 0x401001, 0x401004]");
        assert_eq!(show(&format!("disasm({code}, nil, {{table: true}}).bytes[1]")), r#"b"H\x89\xe5""#);
        assert_eq!(show(r#"disasm(b"\x55", 0, {bits: 32, table: true}).text"#), r#"["push ebp"]"#);
        let want = "401000:  55                     push   rbp\n401001:  48 89 e5               mov    rbp,rsp\n401004:  c3                     ret";
        let crate::value::Value::Str(got) = eval(&format!("disasm({code}, 0x401000)")) else { panic!() };
        assert_eq!(&*got, want);
        let b = elf();
        assert!(try_eval(&format!(r#"{b}.disasm("main")"#)).unwrap_err().msg.contains("read from bytes"));
        assert!(try_eval(&format!(r#"disasm({}, "main")"#, &b[7..b.len() - 1])).unwrap_err().msg.contains(r#"no symbol "main"; the file is stripped"#));
    }

    #[test]
    fn asm() {
        assert_eq!(show(r#"asm("push rbp; mov rbp, rsp\n ret")"#), r#"b"UH\x89\xe5\xc3""#);
        assert_eq!(show(r#"asm("mov qword ptr [rsp + 8], 0x41").disasm(nil, {table: true}).text"#), r#"["mov qword ptr [rsp+8],41h"]"#);
        assert_eq!(show(r#"asm("lea rax, [rbx + rcx*8 - 0x10]").disasm(nil, {table: true}).text"#), r#"["lea rax,[rbx+rcx*8-10h]"]"#);
        assert_eq!(show(r#"asm("add eax, 1; add eax, 0x1000").len"#), "8");
        assert_eq!(show(r#"asm("jmp 0x401002", 0x401000)"#), r#"b"\xeb\0""#);
        assert_eq!(show(r#"asm("jz 0x402000", 0x401000).disasm(0x401000, {table: true}).text"#), r#"["je 0000000000402000h"]"#);
        assert_eq!(show(r#"asm("lea rsi, [rip + 0x10]").disasm(nil, {table: true}).text"#), r#"["lea rsi,[17h]"]"#);
        assert_eq!(show(r#"asm("int 0x80; push ebx", 0, {bits: 32})"#), r#"b"\xcd\x80S""#);
        assert_eq!(
            show(r#"asm("rep movsb; lock inc dword ptr [rax]; syscall").disasm(nil, {table: true}).text"#),
            r#"["rep movsb [rdi],[rsi]", "lock inc dword ptr [rax]", "syscall"]"#
        );
        assert!(try_eval(r#"asm("inc [rax]")"#).unwrap_err().msg.contains("ambiguous"));
        assert!(try_eval(r#"asm("frob eax")"#).unwrap_err().msg.contains(r#"unknown instruction "frob""#));
        assert!(try_eval(r#"asm("mov eax, rbx")"#).unwrap_err().msg.contains("doesn't take these operands"));
    }

    #[test]
    fn gadgets() {
        // `pop rdi; ret` yields the two-instruction gadget and the bare `ret` one byte later.
        assert_eq!(show(r#"gadgets(b"\x5f\xc3").text"#), r#"["pop rdi; ret", "ret"]"#);
        assert_eq!(show(r#"gadgets(b"\x5f\xc3").addr"#), "[0x0, 0x1]");
        assert_eq!(show(r#"gadgets(b"\x5f\xc3", {depth: 1}).text"#), r#"["ret"]"#);
        // An indirect `jmp rax` is a gadget only with jop on.
        assert_eq!(show(r#"gadgets(b"\xff\xe0").len"#), "0");
        assert_eq!(show(r#"gadgets(b"\xff\xe0", {jop: true}).text"#), r#"["jmp rax"]"#);
    }
}
