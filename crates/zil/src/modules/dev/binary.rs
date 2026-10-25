//! Executables: ELF, PE and Mach-O headers, tables and hardening, via the `object` crate.

use crate::ast::Radix;
use crate::interp::Interp;
use crate::lexer::Span;
use crate::modules::{Call, Doc, Fail, Module, doc};
use crate::value::{Table, Value};
use indexmap::IndexMap;
use object::read::elf::{Dyn, ElfFile, FileHeader, ProgramHeader};
use object::read::macho::MachHeader;
use object::read::pe::{ImageNtHeaders, ImageOptionalHeader};
use object::{Endianness, Object, ObjectKind, ObjectSection, ObjectSegment, ObjectSymbol, elf, macho, pe};

pub const MODULE: Module = Module {
    name: "binaries",
    about: "look inside compiled programs and libraries for Linux, Windows and macOS",
    #[rustfmt::skip]
    intro: &[
        ("", "\
Compiled programs and libraries are stored in one of three file formats: ELF on Linux and most other Unix systems, PE \
on Windows (`.exe` and `.dll` files), and Mach-O on macOS and iOS. `binary` reads any of them into a map, so you can \
see what a file is and how it was built without tools like readelf, objdump or otool. `help(\"binary\")` lists the \
map's keys."),
    ],
    fns: FNS,
    call,
    ..Module::EMPTY
};

#[rustfmt::skip]
const FNS: &[Doc] = &[
    doc("binary", "binary(file: str|bytes) -> map", "read a compiled program or library (ELF, PE or Mach-O) into a map of what it is, what's inside it and which security protections it was built with",
        &[], &["read_bytes", "strings", "xxd"]).shown(&[r#"binary("a.out").arch"#, r#"binary("a.out").imports.name"#, r#"binary(read_bytes("app.exe")).nx"#])
        .detail("Takes a path or the file's bytes")
        .fields(&[
            ("format", "str", "\"elf\", \"pe\" or \"macho\""),
            ("arch", "str", "the CPU it's built for, like \"x86_64\" or \"aarch64\""),
            ("bits", "int", "32 or 64"),
            ("endian", "str", "byte order, \"little\" or \"big\""),
            ("entry", "int", "the address where the program starts running"),
            ("sections", "table", "the named pieces of the file, like `.text` for machine code and `.data` for globals. Columns name, addr, size, offset"),
            ("segments", "table", "how sections are loaded into memory. Columns name, addr, size, offset and perm, whether it's readable, writable or executable, like \"r-x\""),
            ("symbols", "table", "the functions and variables the file defines. Columns name, addr, size, kind"),
            ("imports", "table", "the functions it borrows from shared libraries. Columns name, library"),
            ("exports", "table", "the functions it offers to other programs. Columns name, addr"),
            ("pie", "bool", "it loads at a random address each run, so an attacker can't know where its code is"),
            ("nx", "bool", "memory holding data can't run as code, so bytes an attacker sneaks in can't execute"),
            ("canary", "bool", "functions check a guard value next to their locals before returning, catching most stack overflows"),
            ("relro", "str?", "\"full\", \"partial\" or \"none\": how much of the library address table is read-only after loading, so it can't be overwritten to redirect calls. nil for PE and Mach-O, which have no RELRO"),
        ]),
];

fn call(_: &mut Interp, name: &'static str, args: &[Value], _: &Span) -> Call {
    match (name, args) {
        ("binary", [Value::Str(p)]) => {
            let data = crate::host::get().read(p).map_err(|e| Fail::Arg(0, format!("cannot read {p:?}: {e}")))?;
            binary(&data)
        }
        ("binary", [Value::Bytes(b)]) => binary(b),
        _ => Err(Fail::BadArgs),
    }
}

fn binary(data: &[u8]) -> Call {
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
            ("perm", Value::str(perm)),
        ])
    });
    // A stripped file keeps only its dynamic symbols.
    let symbols = if f.symbols().next().is_some() { f.symbols() } else { f.dynamic_symbols() };
    let symbols = symbols.filter(|s| s.is_definition() && !s.name().unwrap_or("").is_empty()).map(|s| {
        let kind = format!("{:?}", s.kind()).to_lowercase();
        row([
            ("name", Value::str(s.name().unwrap_or(""))),
            ("addr", hex(s.address())),
            ("size", int(s.size())),
            ("kind", Value::str(kind)),
            ("global", Value::Bool(s.is_global())),
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
        ("format", Value::str(format)),
        ("arch", Value::str(format!("{:?}", f.architecture()).to_lowercase())),
        ("bits", Value::int(if f.is_64() { 64 } else { 32 })),
        ("endian", Value::str(if f.is_little_endian() { "little" } else { "big" })),
        ("entry", hex(f.entry())),
        ("sections", table(sections)?),
        ("segments", table(segments)?),
        ("symbols", table(symbols)?),
        ("imports", table(imports.iter().map(|i| row([("name", name(i.name())), ("library", text(i.library()))])))?),
        ("exports", table(exports)?),
        ("pie", Value::Bool(pie)),
        ("nx", Value::Bool(nx)),
        ("canary", Value::Bool(canary)),
        ("relro", relro),
    ];
    Ok(Value::map(fields.into_iter().map(|(k, v)| (k.to_string(), v)).collect()))
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
        Value::Int(n, _) => Value::Int(n, Radix { base: 16, width: 0 }),
        Value::Big(n, _) => Value::Big(n, Radix { base: 16, width: 0 }),
        v => v,
    }
}

#[cfg(test)]
mod tests {
    use crate::interp::tests::{show, try_eval};

    /// A 64-bit little-endian x86-64 PIE with a non-executable stack, a read-only load segment and no sections.
    fn elf() -> String {
        let mut b = vec![0x7f, b'E', b'L', b'F', 2, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        let put = |b: &mut Vec<u8>, n: u64, w: usize| b.extend(&n.to_le_bytes()[..w]);
        // e_type DYN, e_machine x86-64, e_version, e_entry, e_phoff, e_shoff, e_flags, e_ehsize, e_phentsize, e_phnum, e_shentsize, e_shnum, e_shstrndx
        for (n, w) in [(3, 2), (62, 2), (1, 4), (0x1040, 8), (64, 8), (0, 8), (0, 4), (64, 2), (56, 2), (2, 2), (64, 2), (0, 2), (0, 2)] {
            put(&mut b, n, w);
        }
        // PT_LOAD r-x over the whole file, then PT_GNU_STACK rw-.
        for (ty, flags, size) in [(1, 5, 176), (0x6474_e551, 6, 0)] {
            for (n, w) in [(ty, 4), (flags, 4), (0, 8), (0, 8), (0, 8), (size, 8), (size, 8), (0x1000, 8)] {
                put(&mut b, n, w);
            }
        }
        format!("binary({:?}.bytes)", b)
    }

    #[test]
    fn binary() {
        let b = elf();
        assert_eq!(show(&format!("x = {b}; [x.format, x.arch, x.bits, x.endian, x.entry]")), r#"["elf", "x86_64", 64, "little", 0x1040]"#);
        assert_eq!(show(&format!("x = {b}; [x.pie, x.nx, x.canary, x.relro]")), r#"[true, true, false, "none"]"#);
        assert_eq!(show(&format!("{b}.segments.perm")), r#"["r-x"]"#);
        assert_eq!(show(&format!("{b}.symbols.len")), "0");
        assert!(try_eval(r#"binary(b"not an executable")"#).unwrap_err().msg.contains("not an ELF, PE or Mach-O file"));
    }
}
