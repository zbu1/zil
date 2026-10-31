//! File forensics: signatures, compression, archives, carving and entropy.

use crate::host::Permission;
use crate::modules::Module;
use crate::value::{Table, Value};
use indexmap::IndexMap;
use miniz_oxide::inflate::TINFLStatus;

pub const MODULE: Module = Module {
    name: "forensics",
    permissions: &[Permission::FilesRead],
    about: "identify files by their contents, unpack compressed data and archives, and find files hidden inside others",
    #[rustfmt::skip]
    intro: &[
        ("", "\
Most file formats start with a few fixed bytes, a signature or \"magic number\", that say what they are whatever the \
file's name. `magic` reads it, `carve` finds every signature inside a larger blob, like a disk image or a firmware \
dump, and `decompress`, `unzip` and `untar` unpack what they find. `entropy_map` shows where data looks random, \
which is usually compressed or encrypted."),
    ],
    #[rustfmt::skip]
    examples: &[
        ("forensics", &[
            ("what kind of file is this?", r#"magic(b"\x89PNG\r\n\x1a\n")"#),
            ("compress and back", r#"gzip(b"hello hello hello").decompress"#),
            ("files hidden in a blob", r#"carve(b"junk" + gzip(b"hi")).name"#),
        ]),
    ],
    fns: fns::FNS,
    #[rustfmt::skip]
    groups: &[
        ("identify", &["magic", "carve", "entropy_map"]),
        ("compression", &["decompress", "inflate", "deflate", "gunzip", "gzip", "unzlib", "zlib", "unxz", "xz", "unzstd", "zstd", "bunzip2", "bzip2"]),
        ("archives", &["unzip", "zip_info", "untar"]),
        ("serialized data", &["flask_session", "php_unserialize", "pickle_dis"]),
    ],
    call: fns::call,
    ..Module::EMPTY
};

#[zil_macros::builtins]
mod fns {
    use super::super::binary::read_path;
    use super::{carved, entropy_blocks, flask, identify, inflated, packed, php, pickle, tar_entries, ungzip, unpacked, zip_entries};
    use crate::modules::Fail;
    use crate::signature::{As, Map, Or};
    use crate::value::{Table, Value};
    use miniz_oxide::deflate::{compress_to_vec, compress_to_vec_zlib};

    /// what kind of file this is, from the signature bytes it starts with, like the `file` command
    ///
    /// Takes a path or bytes, and returns {name, mime, ext}, or nil when no signature matches. Only the contents count, never the file's name
    #[builtin(examples = [r#"magic(b"\x89PNG\r\n\x1a\n")"#, r#"magic(b"PK\x03\x04").ext"#, r#"magic(b"hello")"#], shown = [r#"magic("download.bin")"#], see = ["mime", "carve"])]
    fn magic(file: Or<&str, &[u8]>) -> Result<Option<As<Map>>, Fail> {
        let data;
        let b = match file {
            Or::A(p) => {
                data = read_path(p)?;
                &data[..]
            }
            Or::B(b) => b,
        };
        Ok(identify(b).map(|(name, mime, ext)| {
            let m = [("name", name), ("mime", mime), ("ext", ext)].map(|(k, v)| (k.to_string(), Value::str(v)));
            Value::map(m.into_iter().collect()).into()
        }))
    }

    /// uncompress bytes in any format this module knows (gzip, zlib, xz, zstd, bzip2, or raw deflate), picked by its signature
    #[builtin(examples = [r#"decompress(gzip(b"hi"))"#, r#"decompress(zlib(b"hi"))"#], see = ["magic", "gunzip", "unzlib", "inflate"])]
    fn decompress(b: &[u8]) -> Result<Value, Fail> {
        let out = match identify(b).map(|s| s.2) {
            Some("gz") => ungzip(b),
            Some("zlib") => inflated(b, true),
            Some(ext @ ("xz" | "zst" | "bz2")) => unpacked(b, ext),
            Some(ext @ ("lz4" | "7z" | "rar")) => Err(format!("{ext} data isn't supported yet")),
            Some("zip") => Err("this is a zip archive, so open it with unzip".into()),
            Some("tar") => Err("this is a tar archive, so open it with untar".into()),
            _ => inflated(b, false).map_err(|_| "not gzip, zlib, xz, zstd, bzip2 or deflate data".into()),
        };
        out.map(Value::bytes).map_err(|e| Fail::Arg(0, e))
    }

    /// uncompress raw deflate data, the format inside zip files, gzip and PNG images
    #[builtin(examples = [r#"inflate(deflate(b"hello"))"#], see = ["deflate", "decompress"])]
    fn inflate(b: &[u8]) -> Result<Value, Fail> {
        inflated(b, false).map(Value::bytes).map_err(|e| Fail::Arg(0, e))
    }

    /// compress bytes with raw deflate, no header or checksum
    #[builtin(examples = [r#"deflate(b"aaaaaaaaaa")"#], see = ["inflate", "gzip", "zlib"])]
    fn deflate(b: &[u8]) -> Value {
        Value::bytes(compress_to_vec(b, 6))
    }

    /// uncompress gzip data, the `.gz` format, checking its CRC-32
    #[builtin(examples = [r#"gunzip(gzip(b"hello"))"#], see = ["gzip", "decompress"])]
    fn gunzip(b: &[u8]) -> Result<Value, Fail> {
        ungzip(b).map(Value::bytes).map_err(|e| Fail::Arg(0, e))
    }

    /// compress bytes into gzip, the `.gz` format
    #[builtin(examples = [r#"gzip(b"hello").len"#], see = ["gunzip", "deflate", "zlib"])]
    fn gzip(b: &[u8]) -> Value {
        let mut out = vec![0x1f, 0x8b, 8, 0, 0, 0, 0, 0, 0, 0xff];
        out.extend(compress_to_vec(b, 6));
        out.extend(crate::modules::text::hash::crc32(b).to_le_bytes());
        out.extend((b.len() as u32).to_le_bytes());
        Value::bytes(out)
    }

    /// uncompress zlib data, deflate with a 2-byte header and an Adler-32 checksum, as in PDFs and PNG chunks
    #[builtin(examples = [r#"unzlib(zlib(b"hello"))"#], see = ["zlib", "decompress"])]
    fn unzlib(b: &[u8]) -> Result<Value, Fail> {
        inflated(b, true).map(Value::bytes).map_err(|e| Fail::Arg(0, e))
    }

    /// compress bytes into zlib, deflate with a 2-byte header and an Adler-32 checksum
    #[builtin(examples = [r#"zlib(b"hello")"#], see = ["unzlib", "deflate", "gzip"])]
    fn zlib(b: &[u8]) -> Value {
        Value::bytes(compress_to_vec_zlib(b, 6))
    }

    /// uncompress xz data, the `.xz` format
    #[builtin(examples = [r#"unxz(xz(b"hello"))"#], see = ["xz", "decompress"])]
    fn unxz(b: &[u8]) -> Result<Value, Fail> {
        unpacked(b, "xz").map(Value::bytes).map_err(|e| Fail::Arg(0, e))
    }

    /// compress bytes into xz, the `.xz` format
    ///
    /// The output is valid xz that any tool reads, but zil's encoder doesn't look for repeats, so it barely shrinks anything
    #[builtin(examples = [r#"xz(b"hello").len"#], see = ["unxz", "zstd", "gzip"])]
    fn xz(b: &[u8]) -> Value {
        Value::bytes(packed(b, "xz"))
    }

    /// uncompress Zstandard data, the `.zst` format
    #[builtin(examples = [r#"unzstd(zstd(b"hello"))"#], see = ["zstd", "decompress"])]
    fn unzstd(b: &[u8]) -> Result<Value, Fail> {
        unpacked(b, "zst").map(Value::bytes).map_err(|e| Fail::Arg(0, e))
    }

    /// compress bytes into Zstandard, the `.zst` format, at its fastest level
    #[builtin(examples = [r#"zstd(b"a" * 100).len"#], see = ["unzstd", "xz", "gzip"])]
    fn zstd(b: &[u8]) -> Value {
        Value::bytes(packed(b, "zst"))
    }

    /// uncompress bzip2 data, the `.bz2` format
    #[builtin(examples = [r#"bunzip2(bzip2(b"hello"))"#], see = ["bzip2", "decompress"])]
    fn bunzip2(b: &[u8]) -> Result<Value, Fail> {
        unpacked(b, "bz2").map(Value::bytes).map_err(|e| Fail::Arg(0, e))
    }

    /// compress bytes into bzip2, the `.bz2` format
    #[builtin(examples = [r#"bzip2(b"a" * 100).len"#], see = ["bunzip2", "xz", "gzip"])]
    fn bzip2(b: &[u8]) -> Value {
        Value::bytes(packed(b, "bz2"))
    }

    /// decode a Flask session cookie into {payload, time, signature}, without checking the signature
    ///
    /// Flask signs its cookies but doesn't encrypt them, so anyone holding one can read what's inside. A cookie starting with . is zlib-compressed, which this undoes. Values Flask tags, like tuples or bytes, stay as the tagged maps it stores
    #[builtin(examples = [r#"flask_session("eyJ1c2VyIjoiYWRhIn0.ZQ8Mpw.c2ln").payload"#], see = ["jwt", "php_unserialize"])]
    fn flask_session(cookie: &str) -> Result<As<Map>, Fail> {
        flask(cookie.trim()).map(Into::into).map_err(|e| Fail::Arg(0, e))
    }

    /// read PHP's serialize() format into zil values
    ///
    /// Arrays with keys 0, 1, 2… become lists and other arrays maps. An object becomes a map of its properties with its class under "__class", and the \0 markers on private and protected property names are dropped. Nothing is run or constructed
    #[builtin(examples = [r#"php_unserialize(b"a:2:{i:0;s:3:\"ada\";i:1;b:1;}")"#, r#"php_unserialize(b"O:4:\"User\":1:{s:4:\"name\";s:3:\"ada\";}")"#], see = ["flask_session", "pickle_dis"])]
    fn php_unserialize(s: Or<&str, &[u8]>) -> Result<Value, Fail> {
        let b = match s {
            Or::A(s) => s.as_bytes(),
            Or::B(b) => b,
        };
        php(b).map_err(|e| Fail::Arg(0, e))
    }

    /// list the opcodes of a Python pickle, like pickletools.dis, as a table of offset, op and arg
    ///
    /// Loading a pickle can run any code, which is how pickle exploits work: watch for GLOBAL or STACK_GLOBAL naming something like os system, followed by REDUCE. This only reads the opcodes, so it's safe on any input
    #[builtin(examples = [r#"pickle_dis(b"\x80\x04K*.")"#, r#"pickle_dis(b"cos\nsystem\n(S'id'\ntR.").arg"#], see = ["php_unserialize", "flask_session"])]
    fn pickle_dis(b: &[u8]) -> Result<Table, Fail> {
        pickle(b).map_err(|e| Fail::Arg(0, e))
    }

    /// the entropy of each block of bytes, 256 by default, to spot compressed or encrypted regions in a file
    ///
    /// Each value goes from 0 bits per byte, every byte the same, to 8 for random data. Plain text sits around 4 to 5, and compressed or encrypted data near 8
    #[builtin(examples = [r#"entropy_map(b"\x00" * 512 + gzip(b"hi"), 256)"#], see = ["entropy", "carve"])]
    fn entropy_map(b: &[u8], block: Option<i64>) -> Result<Vec<Value>, Fail> {
        let n = usize::try_from(block.unwrap_or(256)).ok().filter(|&n| n > 0).ok_or_else(|| Fail::Arg(1, "the block size must be positive".into()))?;
        Ok(entropy_blocks(b, n).into_iter().map(Value::Float).collect())
    }

    /// the files in a zip archive as a table of name, size, data, crc, encrypted and comment
    ///
    /// Stored and deflated entries are unpacked, and data is nil for encrypted ones. Folders are rows with a name ending in `/`
    #[builtin(examples = [r#"unzip(b"PK\x05\x06" + b"\x00" * 18)"#], see = ["zip_info", "untar", "decompress"])]
    fn unzip(b: &[u8]) -> Result<Table, Fail> {
        zip_entries(b, true).map_err(|e| Fail::Arg(0, e))
    }

    /// the files listed in a zip archive, without unpacking them: name, size, packed, method, crc, encrypted and comment
    #[builtin(examples = [r#"zip_info(b"PK\x05\x06" + b"\x00" * 18).len"#], see = ["unzip"])]
    fn zip_info(b: &[u8]) -> Result<Table, Fail> {
        zip_entries(b, false).map_err(|e| Fail::Arg(0, e))
    }

    /// the files in a tar archive as a table of name, size and data
    ///
    /// Only regular files are listed. A `.tar.gz` needs gunzip first
    #[builtin(examples = [r#"untar(b"\x00" * 1024).len"#], see = ["unzip", "gunzip"])]
    fn untar(b: &[u8]) -> Result<Table, Fail> {
        tar_entries(b).map_err(|e| Fail::Arg(0, e))
    }

    /// find the files embedded in bytes, like binwalk, by searching for every signature magic() knows
    ///
    /// Returns a table of offset, name, ext and size, the length of the embedded file when its format records where it ends, else nil. Short signatures are skipped, since they match by chance. Slice a hit out with `b[offset:offset + size]`
    #[builtin(examples = [r#"carve(b"junk" + gzip(b"hi") + b"\x89PNG\r\n\x1a\n")"#], see = ["magic", "entropy_map", "decompress"])]
    fn carve(b: &[u8]) -> Table {
        carved(b)
    }
}

/// Name, MIME type, extension and the (offset, bytes) pieces that must all match.
type Sig = (&'static str, &'static str, &'static str, &'static [(usize, &'static [u8])]);

#[rustfmt::skip]
const SIGS: &[Sig] = &[
    ("PNG image", "image/png", "png", &[(0, b"\x89PNG\r\n\x1a\n")]),
    ("JPEG image", "image/jpeg", "jpg", &[(0, b"\xff\xd8\xff")]),
    ("GIF image", "image/gif", "gif", &[(0, b"GIF8")]),
    ("WebP image", "image/webp", "webp", &[(0, b"RIFF"), (8, b"WEBP")]),
    ("TIFF image", "image/tiff", "tif", &[(0, b"II*\x00")]),
    ("TIFF image", "image/tiff", "tif", &[(0, b"MM\x00*")]),
    ("Photoshop image", "image/vnd.adobe.photoshop", "psd", &[(0, b"8BPS")]),
    ("BMP image", "image/bmp", "bmp", &[(0, b"BM")]),
    ("PDF document", "application/pdf", "pdf", &[(0, b"%PDF-")]),
    ("RTF document", "application/rtf", "rtf", &[(0, b"{\\rtf")]),
    ("Microsoft Office document (OLE)", "application/x-ole-storage", "doc", &[(0, b"\xd0\xcf\x11\xe0\xa1\xb1\x1a\xe1")]),
    ("XML document", "application/xml", "xml", &[(0, b"<?xml")]),
    ("SQLite database", "application/vnd.sqlite3", "sqlite", &[(0, b"SQLite format 3\x00")]),
    ("zip archive", "application/zip", "zip", &[(0, b"PK\x03\x04")]),
    ("zip archive", "application/zip", "zip", &[(0, b"PK\x05\x06")]),
    ("tar archive", "application/x-tar", "tar", &[(257, b"ustar")]),
    ("7-Zip archive", "application/x-7z-compressed", "7z", &[(0, b"7z\xbc\xaf\x27\x1c")]),
    ("RAR archive", "application/vnd.rar", "rar", &[(0, b"Rar!\x1a\x07")]),
    ("gzip data", "application/gzip", "gz", &[(0, b"\x1f\x8b\x08")]),
    ("bzip2 data", "application/x-bzip2", "bz2", &[(0, b"BZh")]),
    ("xz data", "application/x-xz", "xz", &[(0, b"\xfd7zXZ\x00")]),
    ("Zstandard data", "application/zstd", "zst", &[(0, b"\x28\xb5\x2f\xfd")]),
    ("LZ4 data", "application/x-lz4", "lz4", &[(0, b"\x04\x22\x4d\x18")]),
    ("zlib data", "application/zlib", "zlib", &[(0, b"\x78\x9c")]),
    ("zlib data", "application/zlib", "zlib", &[(0, b"\x78\xda")]),
    ("zlib data", "application/zlib", "zlib", &[(0, b"\x78\x01")]),
    ("zlib data", "application/zlib", "zlib", &[(0, b"\x78\x5e")]),
    ("ELF executable", "application/x-executable", "elf", &[(0, b"\x7fELF")]),
    ("Mach-O executable", "application/x-mach-binary", "macho", &[(0, b"\xcf\xfa\xed\xfe")]),
    ("Mach-O executable", "application/x-mach-binary", "macho", &[(0, b"\xce\xfa\xed\xfe")]),
    ("Mach-O executable", "application/x-mach-binary", "macho", &[(0, b"\xfe\xed\xfa\xcf")]),
    ("Mach-O executable", "application/x-mach-binary", "macho", &[(0, b"\xfe\xed\xfa\xce")]),
    ("Windows executable (PE)", "application/vnd.microsoft.portable-executable", "exe", &[(0, b"MZ")]),
    ("Java class", "application/java-vm", "class", &[(0, b"\xca\xfe\xba\xbe")]),
    ("WebAssembly module", "application/wasm", "wasm", &[(0, b"\x00asm")]),
    ("pcap capture", "application/vnd.tcpdump.pcap", "pcap", &[(0, b"\xd4\xc3\xb2\xa1")]),
    ("pcap capture", "application/vnd.tcpdump.pcap", "pcap", &[(0, b"\xa1\xb2\xc3\xd4")]),
    ("pcapng capture", "application/vnd.tcpdump.pcap", "pcapng", &[(0, b"\x0a\x0d\x0d\x0a")]),
    ("MP3 audio", "audio/mpeg", "mp3", &[(0, b"ID3")]),
    ("FLAC audio", "audio/flac", "flac", &[(0, b"fLaC")]),
    ("Ogg media", "audio/ogg", "ogg", &[(0, b"OggS")]),
    ("WAV audio", "audio/wav", "wav", &[(0, b"RIFF"), (8, b"WAVE")]),
    ("AVI video", "video/x-msvideo", "avi", &[(0, b"RIFF"), (8, b"AVI ")]),
    ("MP4 video", "video/mp4", "mp4", &[(4, b"ftyp")]),
    ("Matroska video", "video/x-matroska", "mkv", &[(0, b"\x1a\x45\xdf\xa3")]),
    ("script", "text/x-shellscript", "sh", &[(0, b"#!")]),
];

fn matches(b: &[u8], at: usize, parts: &[(usize, &[u8])]) -> bool {
    parts.iter().all(|&(off, m)| b.get(at + off..at + off + m.len()) == Some(m))
}

/// The name, MIME type and extension of the first signature `b` starts with.
fn identify(b: &[u8]) -> Option<(&'static str, &'static str, &'static str)> {
    SIGS.iter().find(|s| matches(b, 0, s.3)).map(|s| (s.0, s.1, s.2))
}

/// The most bytes any decompression may produce.
fn max_out() -> usize {
    // ponytail: one cap for every decompression; a per-call `max` argument if 256 MB is ever too small.
    crate::state::with(|s| s.max_len.get()).min(256 << 20)
}

/// Raw deflate or zlib data, uncompressed up to the session's size cap.
fn inflated(b: &[u8], zlib: bool) -> Result<Vec<u8>, String> {
    let max = max_out();
    let out = if zlib { miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(b, max) } else { miniz_oxide::inflate::decompress_to_vec_with_limit(b, max) };
    out.map_err(|e| match e.status {
        TINFLStatus::HasMoreOutput => crate::state::check_len(max + 1, "bytes").err().unwrap_or_default(),
        _ => format!("not valid {} data", if zlib { "zlib" } else { "deflate" }),
    })
}

/// A writer that fails once more than `max` bytes come in.
struct Capped {
    out: Vec<u8>,
    max: usize,
    full: bool,
}

impl std::io::Write for Capped {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
        if self.out.len() + b.len() > self.max {
            self.full = true;
            return Err(std::io::ErrorKind::FileTooLarge.into());
        }
        self.out.extend_from_slice(b);
        Ok(b.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// xz, zstd or bzip2 data (by `ext`), uncompressed up to the session's size cap.
fn unpacked(b: &[u8], ext: &str) -> Result<Vec<u8>, String> {
    let mut w = Capped { out: vec![], max: max_out(), full: false };
    let ok = match ext {
        "xz" => lzma_rs::xz_decompress(&mut &b[..], &mut w).is_ok(),
        "zst" => ruzstd::decoding::StreamingDecoder::new(b).is_ok_and(|mut r| std::io::copy(&mut r, &mut w).is_ok()),
        _ => std::io::copy(&mut bzip2::read::MultiBzDecoder::new(b), &mut w).is_ok(),
    };
    match (ok, w.full) {
        (_, true) => Err(crate::state::check_len(w.max + 1, "bytes").err().unwrap_or_default()),
        (true, _) => Ok(w.out),
        _ => Err(format!(
            "not valid {} data",
            match ext {
                "zst" => "zstd",
                "bz2" => "bzip2",
                e => e,
            }
        )),
    }
}

/// `b` compressed as xz, zstd or bzip2 (by `ext`).
fn packed(b: &[u8], ext: &str) -> Vec<u8> {
    let mut out = vec![];
    match ext {
        "xz" => lzma_rs::xz_compress(&mut &b[..], &mut out).expect("writing to a Vec can't fail"),
        "zst" => out = ruzstd::encoding::compress_to_vec(b, ruzstd::encoding::CompressionLevel::Fastest),
        _ => {
            std::io::copy(&mut bzip2::read::BzEncoder::new(b, bzip2::Compression::default()), &mut out).expect("writing to a Vec can't fail");
        }
    }
    out
}

/// A gzip member's data, after its header and checked against the CRC-32 in its trailer.
fn ungzip(b: &[u8]) -> Result<Vec<u8>, String> {
    let bad = || "not valid gzip data".to_string();
    if b.len() < 18 || b[..3] != [0x1f, 0x8b, 8] {
        return Err(bad());
    }
    let flags = b[3];
    let mut at = 10;
    if flags & 4 != 0 {
        at += 2 + u16::from_le_bytes([b[10], b[11]]) as usize;
    }
    // The file name, then the comment, each NUL-terminated.
    for bit in [8, 16] {
        if flags & bit != 0 {
            at += b.get(at..).and_then(|r| r.iter().position(|&c| c == 0)).ok_or_else(bad)? + 1;
        }
    }
    if flags & 2 != 0 {
        at += 2;
    }
    let body = b.get(at..b.len() - 8).ok_or_else(bad)?;
    let out = inflated(body, false)?;
    let crc = u32_at(b, b.len() - 8).ok_or_else(bad)? as u32;
    if crate::modules::text::hash::crc32(&out) != crc {
        return Err("the gzip data's CRC-32 doesn't match, so it's corrupt".into());
    }
    Ok(out)
}

fn entropy_blocks(b: &[u8], n: usize) -> Vec<f64> {
    b.chunks(n).map(crate::modules::text::encoding::shannon).collect()
}

fn u16_at(b: &[u8], at: usize) -> Option<usize> {
    Some(u16::from_le_bytes(b.get(at..at + 2)?.try_into().ok()?) as usize)
}

fn u32_at(b: &[u8], at: usize) -> Option<usize> {
    Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?) as usize)
}

/// The entries of a zip's central directory; with `data`, each one unpacked.
fn zip_entries(b: &[u8], data: bool) -> Result<Table, String> {
    let bad = || "not a valid zip archive".to_string();
    // The end-of-central-directory record is the last thing in the file, before a comment of up to 64 KB.
    let eocd = (0..=b.len().saturating_sub(22)).rev().take(22 + 0xffff).find(|&i| b[i..].starts_with(b"PK\x05\x06")).ok_or_else(bad)?;
    let (count, mut at) = (u16_at(b, eocd + 10).ok_or_else(bad)?, u32_at(b, eocd + 16).ok_or_else(bad)?);
    if count == 0xffff || at == 0xffff_ffff {
        return Err("zip64 archives aren't supported".into());
    }
    let mut rows = vec![];
    for _ in 0..count {
        if !b.get(at..).is_some_and(|r| r.starts_with(b"PK\x01\x02")) {
            return Err(bad());
        }
        let f = |off| u16_at(b, at + off).ok_or_else(bad);
        let (flags, method, nlen, elen, clen) = (f(8)?, f(10)?, f(28)?, f(30)?, f(32)?);
        let g = |off| u32_at(b, at + off).ok_or_else(bad);
        let (crc, packed, size, local) = (g(16)?, g(20)?, g(24)?, g(42)?);
        let name = String::from_utf8_lossy(b.get(at + 46..at + 46 + nlen).ok_or_else(bad)?).into_owned();
        let comment = String::from_utf8_lossy(b.get(at + 46 + nlen + elen..at + 46 + nlen + elen + clen).ok_or_else(bad)?).into_owned();
        let encrypted = flags & 1 != 0;
        let int = |n: usize| Value::int(n as i64);
        let crc_v = Value::Int(crc as i64, crate::ast::Radix::HEX);
        if data {
            let body = if encrypted {
                Value::Nil
            } else {
                // The local header repeats the name and has its own extra field.
                let start = local + 30 + u16_at(b, local + 26).ok_or_else(bad)? + u16_at(b, local + 28).ok_or_else(bad)?;
                let raw = b.get(start..start + packed).ok_or_else(bad)?;
                let out = match method {
                    0 => raw.to_vec(),
                    8 => inflated(raw, false)?,
                    m => return Err(format!("{name:?} is packed with method {m}, which isn't supported (only stored and deflate are)")),
                };
                if crate::modules::text::hash::crc32(&out) != crc as u32 {
                    return Err(format!("{name:?} doesn't match its CRC-32, so the archive is corrupt"));
                }
                Value::bytes(out)
            };
            rows.push(vec![Value::str(name), int(size), body, crc_v, Value::Bool(encrypted), Value::str(comment)]);
        } else {
            let how = match method {
                0 => "stored".to_string(),
                8 => "deflate".to_string(),
                m => m.to_string(),
            };
            rows.push(vec![Value::str(name), int(size), int(packed), Value::str(how), crc_v, Value::Bool(encrypted), Value::str(comment)]);
        }
        at += 46 + nlen + elen + clen;
    }
    let cols: &[&str] =
        if data { &["name", "size", "data", "crc", "encrypted", "comment"] } else { &["name", "size", "packed", "method", "crc", "encrypted", "comment"] };
    Ok(Table { cols: cols.iter().map(|c| c.to_string()).collect(), rows })
}

/// The regular files of a tar archive.
fn tar_entries(b: &[u8]) -> Result<Table, String> {
    let mut rows = vec![];
    let mut at = 0;
    // ponytail: no GNU long names or pax headers; names past 100 bytes (155 with the ustar prefix) come out cut.
    while let Some(h) = b.get(at..at + 512) {
        if h.iter().all(|&c| c == 0) {
            break;
        }
        let text = |r: std::ops::Range<usize>| String::from_utf8_lossy(&h[r]).trim_end_matches('\0').to_string();
        let size = usize::from_str_radix(text(124..136).trim(), 8).map_err(|_| format!("a tar header at offset {at:#x} has no valid size"))?;
        let name = match text(345..500) {
            p if h[257..262] == *b"ustar" && !p.is_empty() => format!("{p}/{}", text(0..100)),
            _ => text(0..100),
        };
        let start = at + 512;
        if matches!(h[156], b'0' | 0) {
            let data = b.get(start..start + size).ok_or_else(|| format!("{name:?} runs past the end of the archive"))?;
            rows.push(vec![Value::str(name), Value::int(size as i64), Value::bytes(data)]);
        }
        at = start + size.div_ceil(512) * 512;
    }
    Ok(Table { cols: ["name", "size", "data"].map(String::from).to_vec(), rows })
}

/// Where the file starting at `s` ends, for formats that record it.
fn carved_len(b: &[u8], s: usize, ext: &str) -> Option<usize> {
    let find = |from: usize, needle: &[u8]| b.get(from..)?.windows(needle.len()).position(|w| w == needle).map(|i| from + i);
    match ext {
        "png" => {
            let mut at = s + 8;
            loop {
                let len = u32::from_be_bytes(b.get(at..at + 4)?.try_into().ok()?) as usize;
                let end = at + 12 + len;
                if b.get(at + 4..at + 8)? == b"IEND" {
                    return (end <= b.len()).then_some(end - s);
                }
                at = end;
            }
        }
        "zip" => find(s, b"PK\x05\x06").and_then(|e| Some(e + 22 + u16_at(b, e + 20)? - s)),
        "pdf" => find(s, b"%%EOF").map(|e| e + 5 - s),
        "jpg" => find(s + 2, b"\xff\xd9").map(|e| e + 2 - s),
        "bmp" => u32_at(b, s + 2),
        "wav" | "avi" | "webp" => u32_at(b, s + 4).map(|n| n + 8),
        _ => None,
    }
    .filter(|&n| s + n <= b.len())
}

fn carved(b: &[u8]) -> Table {
    let mut rows = vec![];
    for s in 0..b.len() {
        // Two-byte signatures, like `MZ` or zlib's, match random data too often.
        if let Some(sig) = SIGS.iter().find(|sig| sig.3.iter().map(|p| p.1.len()).sum::<usize>() > 2 && matches(b, s, sig.3)) {
            let len = carved_len(b, s, sig.2).map_or(Value::Nil, |n| Value::int(n as i64));
            rows.push(vec![crate::modules::text::encoding::hex_int(s), Value::str(sig.0), Value::str(sig.2), len]);
        }
    }
    Table { cols: ["offset", "name", "ext", "size"].map(String::from).to_vec(), rows }
}

/// A Flask session cookie as {payload, time, signature}.
fn flask(cookie: &str) -> Result<Value, String> {
    let b64 = |p: &str| base64::Engine::decode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, p.trim_end_matches('=')).ok();
    let (zipped, rest) = cookie.strip_prefix('.').map_or((false, cookie), |r| (true, r));
    let [payload, time, signature] = rest.split('.').collect::<Vec<_>>()[..] else {
        return Err("expected a Flask session cookie: payload.time.signature".into());
    };
    let mut payload = b64(payload).ok_or("invalid base64url in the payload")?;
    if zipped {
        payload = inflated(&payload, true)?;
    }
    let payload = serde_json::from_slice(&payload).map_err(|_| "the payload is not JSON")?;
    let secs = b64(time).filter(|t| t.len() <= 8).map(|t| t.iter().fold(0i64, |n, &x| n << 8 | x as i64));
    let time = secs.and_then(|s| jiff::Timestamp::from_second(s).ok()).map_or(Value::Nil, |t| Value::date(t.to_zoned(crate::host::tz())));
    let fields = [("payload", crate::modules::data::formats::from_json(payload)), ("time", time), ("signature", Value::str(signature))];
    Ok(Value::map(fields.into_iter().map(|(k, v)| (k.to_string(), v)).collect()))
}

/// A value in PHP's serialize() format.
fn php(b: &[u8]) -> Result<Value, String> {
    let mut at = 0;
    let v = php_value(b, &mut at, 0)?;
    if at < b.trim_ascii_end().len() {
        return Err(format!("unexpected data after the value at byte {at}"));
    }
    Ok(v)
}

fn php_value(b: &[u8], at: &mut usize, depth: usize) -> Result<Value, String> {
    if depth > 256 {
        return Err("PHP serialized data nested more than 256 deep".into());
    }
    let bad = |at: usize| format!("not valid PHP serialized data at byte {at}");
    // The text up to `end`, which is consumed.
    let until = |at: &mut usize, end: u8| -> Result<String, String> {
        let n = b.get(*at..).and_then(|r| r.iter().position(|&c| c == end)).ok_or_else(|| bad(*at))?;
        let s = String::from_utf8_lossy(&b[*at..*at + n]).into_owned();
        *at += n + 1;
        Ok(s)
    };
    let expect = |at: &mut usize, s: &[u8]| -> Result<(), String> {
        if b.get(*at..*at + s.len()) != Some(s) {
            return Err(bad(*at));
        }
        *at += s.len();
        Ok(())
    };
    // A length-prefixed "…" string, after its `s:`.
    let quoted = |at: &mut usize| -> Result<String, String> {
        let n: usize = until(at, b':')?.parse().map_err(|_| bad(*at))?;
        expect(at, b"\"")?;
        let s = String::from_utf8_lossy(b.get(*at..at.saturating_add(n)).ok_or_else(|| bad(*at))?).into_owned();
        *at += n;
        expect(at, b"\"")?;
        Ok(s)
    };
    let kind = *b.get(*at).ok_or_else(|| bad(*at))?;
    if kind == b'N' {
        expect(at, b"N;")?;
        return Ok(Value::Nil);
    }
    *at += 1;
    expect(at, b":")?;
    Ok(match kind {
        b'b' => Value::Bool(until(at, b';')? == "1"),
        b'i' => Value::int(until(at, b';')?.parse().map_err(|_| bad(*at))?),
        b'd' => Value::Float(match until(at, b';')?.as_str() {
            "INF" => f64::INFINITY,
            "-INF" => f64::NEG_INFINITY,
            "NAN" => f64::NAN,
            x => x.parse().map_err(|_| bad(*at))?,
        }),
        b's' | b'E' => {
            let s = quoted(at)?;
            expect(at, b";")?;
            Value::str(s)
        }
        b'a' | b'O' => {
            let class = if kind == b'O' { Some(quoted(at).and_then(|c| expect(at, b":").map(|_| c))?) } else { None };
            let n: usize = until(at, b':')?.parse().map_err(|_| bad(*at))?;
            expect(at, b"{")?;
            let mut items = IndexMap::new();
            if let Some(c) = class {
                items.insert("__class".to_string(), Value::str(c));
            }
            let mut list = kind == b'a';
            for i in 0..n {
                let k = php_value(b, at, depth + 1)?;
                list &= matches!(k, Value::Int(k, _) if k == i as i64);
                // Private and protected properties are named \0Class\0name and \0*\0name.
                let k = k.to_string();
                items.insert(k.rsplit('\0').next().unwrap_or_default().to_string(), php_value(b, at, depth + 1)?);
            }
            expect(at, b"}")?;
            if list { Value::list(items.into_values().collect()) } else { Value::map(items) }
        }
        b'r' | b'R' | b'C' => return Err(format!("{:?} values (references and custom serializers) aren't supported", kind as char)),
        _ => return Err(bad(*at - 2)),
    })
}

/// How a pickle opcode's argument is stored.
#[derive(Clone, Copy)]
enum Arg {
    None,
    /// An unsigned little-endian integer of this many bytes.
    Uint(usize),
    I4,
    /// Text up to a newline.
    Line,
    /// Two newline-terminated lines, a module and a name.
    Lines2,
    Float8,
    /// A little-endian signed integer, its length in 1 or 4 bytes first.
    Long(usize),
    /// Text, its length in 1, 4 or 8 bytes first.
    Text(usize),
    /// Bytes, their length in 1, 4 or 8 bytes first.
    Bytes(usize),
}

#[rustfmt::skip]
const PICKLE_OPS: &[(u8, &str, Arg)] = &[
    (b'(', "MARK", Arg::None), (b'.', "STOP", Arg::None), (b'0', "POP", Arg::None), (b'1', "POP_MARK", Arg::None),
    (b'2', "DUP", Arg::None), (b'F', "FLOAT", Arg::Line), (b'I', "INT", Arg::Line), (b'J', "BININT", Arg::I4),
    (b'K', "BININT1", Arg::Uint(1)), (b'L', "LONG", Arg::Line), (b'M', "BININT2", Arg::Uint(2)), (b'N', "NONE", Arg::None),
    (b'P', "PERSID", Arg::Line), (b'Q', "BINPERSID", Arg::None), (b'R', "REDUCE", Arg::None), (b'S', "STRING", Arg::Line),
    (b'T', "BINSTRING", Arg::Text(4)), (b'U', "SHORT_BINSTRING", Arg::Text(1)), (b'V', "UNICODE", Arg::Line),
    (b'X', "BINUNICODE", Arg::Text(4)), (b'a', "APPEND", Arg::None), (b'b', "BUILD", Arg::None), (b'c', "GLOBAL", Arg::Lines2),
    (b'd', "DICT", Arg::None), (b'}', "EMPTY_DICT", Arg::None), (b'e', "APPENDS", Arg::None), (b'g', "GET", Arg::Line),
    (b'h', "BINGET", Arg::Uint(1)), (b'i', "INST", Arg::Lines2), (b'j', "LONG_BINGET", Arg::Uint(4)), (b'l', "LIST", Arg::None),
    (b']', "EMPTY_LIST", Arg::None), (b'o', "OBJ", Arg::None), (b'p', "PUT", Arg::Line), (b'q', "BINPUT", Arg::Uint(1)),
    (b'r', "LONG_BINPUT", Arg::Uint(4)), (b's', "SETITEM", Arg::None), (b't', "TUPLE", Arg::None), (b')', "EMPTY_TUPLE", Arg::None),
    (b'u', "SETITEMS", Arg::None), (b'G', "BINFLOAT", Arg::Float8), (0x80, "PROTO", Arg::Uint(1)), (0x81, "NEWOBJ", Arg::None),
    (0x82, "EXT1", Arg::Uint(1)), (0x83, "EXT2", Arg::Uint(2)), (0x84, "EXT4", Arg::I4), (0x85, "TUPLE1", Arg::None),
    (0x86, "TUPLE2", Arg::None), (0x87, "TUPLE3", Arg::None), (0x88, "NEWTRUE", Arg::None), (0x89, "NEWFALSE", Arg::None),
    (0x8a, "LONG1", Arg::Long(1)), (0x8b, "LONG4", Arg::Long(4)), (b'B', "BINBYTES", Arg::Bytes(4)),
    (b'C', "SHORT_BINBYTES", Arg::Bytes(1)), (0x8c, "SHORT_BINUNICODE", Arg::Text(1)), (0x8d, "BINUNICODE8", Arg::Text(8)),
    (0x8e, "BINBYTES8", Arg::Bytes(8)), (0x8f, "EMPTY_SET", Arg::None), (0x90, "ADDITEMS", Arg::None),
    (0x91, "FROZENSET", Arg::None), (0x92, "NEWOBJ_EX", Arg::None), (0x93, "STACK_GLOBAL", Arg::None),
    (0x94, "MEMOIZE", Arg::None), (0x95, "FRAME", Arg::Uint(8)), (0x96, "BYTEARRAY8", Arg::Bytes(8)),
    (0x97, "NEXT_BUFFER", Arg::None), (0x98, "READONLY_BUFFER", Arg::None),
];

/// A pickle's opcodes, up to and including STOP.
fn pickle(b: &[u8]) -> Result<Table, String> {
    let mut rows = vec![];
    let mut at = 0;
    loop {
        let start = at;
        let &code = b.get(at).ok_or("the pickle ends without a STOP opcode")?;
        let &(_, name, arg) = PICKLE_OPS.iter().find(|op| op.0 == code).ok_or_else(|| format!("unknown pickle opcode 0x{code:02x} at byte {at}"))?;
        at += 1;
        let mut take = |n: usize| -> Result<&[u8], String> {
            let r = b.get(at..at.saturating_add(n)).ok_or_else(|| format!("the pickle ends inside the {name} opcode at byte {start}"))?;
            at += n;
            Ok(r)
        };
        let uint = |r: &[u8]| r.iter().rev().fold(0u64, |n, &x| n << 8 | x as u64);
        let value = match arg {
            Arg::None => Value::Nil,
            Arg::Uint(n) => crate::modules::math::big(uint(take(n)?).into()),
            Arg::I4 => Value::int(i32::from_le_bytes(take(4)?.try_into().expect("4 bytes")) as i64),
            Arg::Line | Arg::Lines2 => {
                let mut lines = vec![];
                for _ in 0..if matches!(arg, Arg::Line) { 1 } else { 2 } {
                    let mut s = vec![];
                    while let c = take(1)?[0]
                        && c != b'\n'
                    {
                        s.push(c);
                    }
                    lines.push(String::from_utf8_lossy(&s).into_owned());
                }
                Value::str(lines.join(" "))
            }
            Arg::Float8 => Value::Float(f64::from_be_bytes(take(8)?.try_into().expect("8 bytes"))),
            Arg::Long(n) | Arg::Text(n) | Arg::Bytes(n) => {
                let len = usize::try_from(uint(take(n)?)).unwrap_or(usize::MAX);
                let data = take(len)?;
                match arg {
                    Arg::Long(_) => crate::modules::math::big(num_bigint::BigInt::from_signed_bytes_le(data)),
                    Arg::Text(_) => Value::str(String::from_utf8_lossy(data)),
                    _ => Value::bytes(data),
                }
            }
        };
        rows.push(vec![crate::modules::text::encoding::hex_int(start), Value::str(name), value]);
        if code == b'.' {
            return Ok(Table { cols: ["offset", "op", "arg"].map(String::from).to_vec(), rows });
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::interp::tests::{show, try_eval};

    #[test]
    fn magic() {
        assert_eq!(show(r#"magic(b"\x89PNG\r\n\x1a\n").name"#), "PNG image");
        assert_eq!(show(r#"magic(b"RIFF\x00\x00\x00\x00WAVEfmt ").ext"#), "wav");
        assert_eq!(show(r#"magic(b"\x00" * 257 + b"ustar").mime"#), "application/x-tar");
        assert_eq!(show(r#"magic(b"hello")"#), "nil");
    }

    #[test]
    fn compression() {
        assert_eq!(show(r#"gunzip(gzip(b"hello hello"))"#), r#"b"hello hello""#);
        assert_eq!(show(r#"unzlib(zlib(b"abc"))"#), r#"b"abc""#);
        assert_eq!(show(r#"inflate(deflate(b"abc"))"#), r#"b"abc""#);
        assert_eq!(show(r#"[gzip(b"x"), zlib(b"x"), deflate(b"x")].map(decompress)"#), r#"[b"x", b"x", b"x"]"#);
        // `gzip -n` of "hi\n", with the name flag set by hand so the header's name is skipped.
        let gz = r#"b"\x1f\x8b\x08\x08\x00\x00\x00\x00\x00\x03a\x00\xcb\xc8\xe4\x02\x00\x7a\x7a\x6f\xed\x03\x00\x00\x00""#;
        assert_eq!(show(&format!("gunzip({gz})")), r#"b"hi\n""#);
        assert!(try_eval(r#"gunzip(patch(gzip(b"hello"), 18, b"\x00"))"#).unwrap_err().msg.contains("CRC-32"));
        assert_eq!(show(r#"[xz(b"hi"), zstd(b"hi"), bzip2(b"hi")].map(decompress)"#), r#"[b"hi", b"hi", b"hi"]"#);
        assert_eq!(show(r#"[unxz(xz(b"")).len, unzstd(zstd(b"ab" * 500)) == b"ab" * 500, bunzip2(bzip2(b"ab" * 500)).len]"#), "[0, true, 1000]");
        assert!(try_eval(r#"decompress(b"\xfd7zXZ\x00")"#).unwrap_err().msg.contains("not valid xz"));
        assert!(try_eval(r#"bunzip2(b"BZh9junk")"#).unwrap_err().msg.contains("not valid bzip2"));
        assert!(try_eval(r#"decompress(b"\x04\x22\x4d\x18")"#).unwrap_err().msg.contains("lz4 data isn't supported"));
        assert!(try_eval(r#"inflate(b"\xff\xff")"#).unwrap_err().msg.contains("not valid deflate"));
    }

    #[test]
    fn decompression_is_capped() {
        let mut it = crate::interp::Interp::new();
        it.limits.len = 1000;
        let ast = crate::parser::parse(r#"gunzip(gzip(b"\x00" * 999) )"#).unwrap();
        assert!(it.run(&ast).is_ok());
        let ast = crate::parser::parse(r#"inflate(deflate(b"\x00" * 900 + b"\x00" * 900))"#).unwrap();
        assert!(it.run(&ast).unwrap_err().msg.contains("too large"));
        for f in ["xz", "zstd", "bzip2"] {
            let ast = crate::parser::parse(&format!(r#"decompress({f}(b"\x00" * 900 + b"\x00" * 900))"#)).unwrap();
            assert!(it.run(&ast).unwrap_err().msg.contains("too large"), "{f}");
        }
    }

    #[test]
    fn entropy_map() {
        assert_eq!(show(r#"entropy_map(b"aaaa" + b"abcd", 4)"#), "[0, 2]");
        assert!(try_eval(r#"entropy_map(b"a", 0)"#).is_err());
    }

    /// A zip of `a.txt` ("hi", stored) and `b.txt` ("hello hello hello", deflated).
    fn zip() -> String {
        let mut b: Vec<u8> = vec![];
        let mut central: Vec<u8> = vec![];
        let entries: [(&str, &[u8], u16); 2] = [("a.txt", b"hi", 0), ("b.txt", b"hello hello hello", 8)];
        for (name, data, method) in entries {
            let packed = if method == 8 { miniz_oxide::deflate::compress_to_vec(data, 6) } else { data.to_vec() };
            let crc = crate::modules::text::hash::crc32(data);
            let local = b.len() as u32;
            let mut common = vec![20, 0, 0, 0];
            common.extend(method.to_le_bytes());
            common.extend([0, 0, 0, 0]);
            common.extend(crc.to_le_bytes());
            common.extend((packed.len() as u32).to_le_bytes());
            common.extend((data.len() as u32).to_le_bytes());
            common.extend((name.len() as u16).to_le_bytes());
            common.extend([0, 0]);
            b.extend(b"PK\x03\x04");
            b.extend(&common);
            b.extend(name.as_bytes());
            b.extend(&packed);
            central.extend(b"PK\x01\x02\x14\x00");
            central.extend(&common);
            // Comment length, disk, internal and external attributes.
            central.extend([0; 10]);
            central.extend(local.to_le_bytes());
            central.extend(name.as_bytes());
        }
        let cd = b.len() as u32;
        let cd_len = central.len() as u32;
        b.extend(central);
        b.extend(b"PK\x05\x06\x00\x00\x00\x00\x02\x00\x02\x00");
        b.extend(cd_len.to_le_bytes());
        b.extend(cd.to_le_bytes());
        b.extend([0, 0]);
        format!("b\"{}\"", b.iter().map(|c| format!("\\x{c:02x}")).collect::<String>())
    }

    #[test]
    fn unzip() {
        let z = zip();
        assert_eq!(show(&format!("unzip({z}).name")), r#"["a.txt", "b.txt"]"#);
        assert_eq!(show(&format!("unzip({z}).data")), r#"[b"hi", b"hello hello hello"]"#);
        assert_eq!(show(&format!("zip_info({z}).method")), r#"["stored", "deflate"]"#);
        assert_eq!(show(&format!("zip_info({z}).size")), "[2, 17]");
        assert_eq!(show(&format!("carve(b\"xx\" + {z})[0]")), format!("{{offset: 0x2, name: \"zip archive\", ext: \"zip\", size: {}}}", (z.len() - 3) / 4));
        assert!(try_eval(r#"unzip(b"nope")"#).unwrap_err().msg.contains("not a valid zip"));
    }

    #[test]
    fn untar() {
        // A header with the name, size (octal), type and ustar magic at their offsets, the data padded to 512, then two zero blocks.
        let z = |n: usize| format!(r#"b"\x00" * {n}"#);
        let t = format!(r#"(b"a.txt" + {} + b"00000000002" + {} + b"0" + {} + b"ustar\x00" + {} + b"hi" + {})"#, z(119), z(21), z(100), z(249), z(1534));
        assert_eq!(show(&format!("untar({t}).name")), r#"["a.txt"]"#);
        assert_eq!(show(&format!("untar({t}).data")), r#"[b"hi"]"#);
        assert_eq!(show(&format!("magic({t}).ext")), "tar");
    }

    #[test]
    fn carve() {
        let png = r#"b"\x89PNG\r\n\x1a\n" + b"\x00\x00\x00\x00IEND\xaeB`\x82""#;
        assert_eq!(show(&format!(r#"carve(b"junk" + {png} + b"tail").len"#)), "1");
        assert_eq!(show(&format!(r#"carve(b"junk" + {png} + b"tail")[0].size"#)), "20");
        assert_eq!(show(r#"carve(b"MZ\x78\x9c").len"#), "0");
    }

    #[test]
    fn flask_session() {
        let c = ".eJyrViotTi1SslJKTElU0lHKU7KKNtQxiq0FAFvfBv8.ZVPxAA.sig";
        assert_eq!(show(&format!(r#"flask_session("{c}").payload"#)), r#"{user: "ada", n: [1, 2]}"#);
        assert_eq!(show(&format!(r#"flask_session("{c}").time.unix"#)), "1700000000");
        assert_eq!(show(r#"flask_session("eyJhIjoxfQ.x.y").signature"#), "y");
        assert!(try_eval(r#"flask_session("abc")"#).is_err());
        assert!(try_eval(r#"flask_session(".eyJhIjoxfQ.x.y")"#).unwrap_err().msg.contains("zlib"));
    }

    #[test]
    fn php_unserialize() {
        assert_eq!(show(r#"php_unserialize("a:3:\{i:0;s:3:\"ada\";i:1;b:1;i:2;N;}")"#), r#"["ada", true, nil]"#);
        assert_eq!(show(r#"php_unserialize("a:2:\{s:1:\"x\";d:1.5;i:5;i:-2;}")"#), "{x: 1.5, 5: -2}");
        let obj = r#"b"O:4:\"User\":2:{s:10:\"\x00User\x00name\";s:3:\"ada\";s:6:\"\x00*\x00age\";i:36;}""#;
        assert_eq!(show(&format!("php_unserialize({obj})")), r#"{__class: "User", name: "ada", age: 36}"#);
        assert_eq!(show(r#"php_unserialize("s:3:\"hé\";")"#), "hé");
        assert!(try_eval(r#"php_unserialize("s:9:\"short\";")"#).is_err());
        assert!(try_eval(r#"php_unserialize("i:1;junk")"#).unwrap_err().msg.contains("after the value"));
        assert!(try_eval(r#"php_unserialize("a:1:\{i:0;r:1;}")"#).unwrap_err().msg.contains("references"));
        assert!(try_eval(&format!(r#"php_unserialize("{}")"#, r"a:1:\{i:0;".repeat(300))).unwrap_err().msg.contains("nested"));
    }

    #[test]
    fn pickle_dis() {
        let p = r#"b"\x80\x04\x955\x00\x00\x00\x00\x00\x00\x00}\x94(\x8c\x01a\x94]\x94(K\x01\x8a\t\x00\x00\x00\x00\x00\x00\x00\x00@G\xc0\x0c\x00\x00\x00\x00\x00\x00C\x01x\x94\x88N\x86\x94e\x8c\x01s\x94\x8c\x03h\xc3\xa9\x94u.""#;
        assert_eq!(show(&format!("pickle_dis({p}).len")), "26");
        assert_eq!(show(&format!("pickle_dis({p})[1]")), "{offset: 0x2, op: \"FRAME\", arg: 53}");
        assert_eq!(show(&format!("pickle_dis({p})[11].arg == 2**70")), "true");
        assert_eq!(show(&format!("pickle_dis({p}).arg[12..14] + [pickle_dis({p})[22].arg]")), r#"[-3.5, b"x", "hé"]"#);
        assert_eq!(show(r#"pickle_dis(b"(lp0\nI1\naVa\np1\na.").op"#), r#"["MARK", "LIST", "PUT", "INT", "APPEND", "UNICODE", "PUT", "APPEND", "STOP"]"#);
        assert_eq!(show(r#"pickle_dis(b"cos\nsystem\n(S'id'\ntR.")[0].arg"#), "os system");
        assert!(try_eval(r#"pickle_dis(b"\x80\x04K")"#).unwrap_err().msg.contains("inside the BININT1"));
        assert!(try_eval(r#"pickle_dis(b"N")"#).unwrap_err().msg.contains("without a STOP"));
        assert!(try_eval(r#"pickle_dis(b"\xff")"#).unwrap_err().msg.contains("0xff"));
        assert!(try_eval(r#"pickle_dis(b"\x8e\xff\xff\xff\xff\xff\xff\xff\xff.")"#).is_err());
    }
}
