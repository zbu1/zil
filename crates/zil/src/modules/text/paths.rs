//! Paths as text: joining them and taking them apart, without touching the file system.

use crate::modules::Module;

pub const MODULE: Module = Module {
    name: "paths",
    tags: &["parsing"],
    about: "join paths and take them apart: folder, file name, extension",
    #[rustfmt::skip]
    intro: &[
        ("", "\
A path is text like `/home/ada/notes.txt`. These functions join paths and take them apart without touching the \
disk, so they work on paths that don't exist."),
    ],
    #[rustfmt::skip]
    examples: &[
        ("paths", &[
            ("a file name without, and then only, its extension", r#"["a/b.tar.gz".stem, "a/b.tar.gz".ext]"#),
        ]),
    ],
    fns: fns::FNS,
    call: fns::call,
    ..Module::EMPTY
};

#[zil_macros::builtins]
mod fns {
    use crate::signature::Rest;
    use std::path::{Path, PathBuf};

    /// join pieces into one path, with / or \ between them as your system uses
    #[builtin(examples = [r#"path_join("a", "b.txt")"#], see = ["dirname", "basename"])]
    fn path_join(a: &str, more: Rest<&str>) -> String {
        let mut out = PathBuf::from(a);
        for p in more.0 {
            out.push(p);
        }
        out.to_string_lossy().into_owned()
    }

    fn part(p: Option<&std::ffi::OsStr>) -> Option<String> {
        p.map(|s| s.to_string_lossy().into_owned())
    }

    /// the last part of a path, usually the file name, or nil
    #[builtin(examples = [r#""a/b/c.txt".basename"#], see = ["dirname", "stem"])]
    fn basename(path: &str) -> Option<String> {
        part(Path::new(path).file_name())
    }

    /// everything before the last part of a path, usually its folder, or nil
    #[builtin(examples = [r#""a/b/c.txt".dirname"#], see = ["basename"])]
    fn dirname(path: &str) -> Option<String> {
        part(Path::new(path).parent().map(|d| d.as_os_str()))
    }

    /// the file's extension without the dot, like "gz", or nil if it has none
    #[builtin(examples = [r#""a/b.tar.gz".ext"#, r#""README".ext"#], see = ["stem"])]
    fn ext(path: &str) -> Option<String> {
        part(Path::new(path).extension())
    }

    /// the file name without its last extension, or nil
    #[builtin(examples = [r#""a/b.tar.gz".stem"#], see = ["ext", "basename"])]
    fn stem(path: &str) -> Option<String> {
        part(Path::new(path).file_stem())
    }
}

#[cfg(test)]
mod tests {
    use crate::interp::tests::show;

    #[test]
    fn paths() {
        assert_eq!(show(r#"["a/b.tar.gz".stem, "a/b.tar.gz".ext, "a/b".dirname, "x".ext, "/".basename]"#), r#"["b.tar", "gz", "a", nil, nil]"#);
    }
}
