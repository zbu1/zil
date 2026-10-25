//! `#[builtins]` turns a module's typed Rust fns into its `FNS` docs and its `call`, so a builtin's signature is its code.

use proc_macro::TokenStream;
use proc_macro2::TokenStream as Tokens;
use quote::{format_ident, quote};
use syn::ext::IdentExt;
use syn::{
    Error, Expr, ExprArray, FnArg, GenericArgument, Ident, Item, ItemFn, ItemMod, Lit, Meta, Pat, PathArguments, ReturnType, Type, UnOp, parse_macro_input,
};

/// On an inline `mod`: each `#[builtin(...)]` fn in it is one form of a builtin. Adds `FNS` and `call` to the mod.
///
/// - Params: `x: T`, `x: Option<T>` (shown `x?: T`), `#[default(0)] x: T` (shown `x: T = 0`). `&mut Interp` and
///   `&Span` params aren't arguments: they get the interpreter and the call's span. A last `xs: Rest<T>` takes the
///   remaining arguments, shown `*xs: T`.
/// - Types are named by `signature::Arg` and `signature::Ret`; `Or<A, B>` (and `Or3` to `Or5`) shows as `A|B`, a returned `Option<T>` as `T|nil`, `Result<T, _>` as `T`,
///   and `As<T>` (a `Value` that's always a `T`) as `T`.
/// - The first `///` paragraph is the desc, the rest the detail.
/// - A `Flags` param takes no argument: it gets the command flags (`rm -rf`) from a trailing map, shown `-rf` from
///   `flags = [("r", "recursive", "meaning"), ("f", "force", "meaning")]`, with `command = "rm -rf build"` as its command-line example.
/// - `aliases = ["cat"]` are other names for it, sharing its help page.
/// - `#[builtin(name = "range", aliases = [...], examples = [...], see = [...], shown = [...], fields = [...], flags = [...], command = "...", mutates, pretty)]`; forms
///   sharing a name are tried in order, and only the first may have docs other than examples and shown.
#[proc_macro_attribute]
pub fn builtins(_: TokenStream, item: TokenStream) -> TokenStream {
    let mut m = parse_macro_input!(item as ItemMod);
    expand(&mut m).unwrap_or_else(|e| e.to_compile_error()).into()
}

#[derive(Default)]
struct Attr {
    name: Option<String>,
    aliases: Option<Expr>,
    examples: Vec<Expr>,
    see: Option<Expr>,
    shown: Vec<Expr>,
    fields: Option<Expr>,
    mutates: bool,
    pretty: bool,
    flags: Option<Vec<(char, String, String)>>,
    command: Option<String>,
}

struct Form {
    name: String,
    desc: String,
    detail: String,
    attr: Attr,
    /// `&'static str` expressions that concatenate to the form's sig text.
    sig: Vec<Tokens>,
    /// Converts the arguments and calls the fn if they fit, returning from `call`.
    attempt: Tokens,
}

fn expand(m: &mut ItemMod) -> syn::Result<Tokens> {
    let Some((_, items)) = &mut m.content else { return Err(Error::new_spanned(&m.ident, "#[builtins] needs an inline mod")) };
    let mut forms: Vec<Form> = vec![];
    for item in items.iter_mut() {
        let Item::Fn(f) = item else { continue };
        let Some(i) = f.attrs.iter().position(|a| a.path().is_ident("builtin")) else { continue };
        let attr = f.attrs.remove(i);
        forms.push(form(f, &attr)?);
    }
    let mut names: Vec<&str> = vec![];
    for f in &forms {
        if !names.contains(&f.name.as_str()) {
            names.push(&f.name);
        }
    }
    let mut docs = vec![];
    let mut arms = vec![];
    for name in names {
        let group: Vec<&Form> = forms.iter().filter(|f| f.name == name).collect();
        let first = group[0];
        let mut sig = vec![];
        for (i, f) in group.iter().enumerate() {
            if i > 0 {
                if !f.desc.is_empty()
                    || f.attr.aliases.is_some()
                    || f.attr.see.is_some()
                    || f.attr.fields.is_some()
                    || f.attr.mutates
                    || f.attr.pretty
                    || f.attr.flags.is_some()
                    || f.attr.command.is_some()
                {
                    return Err(Error::new(
                        proc_macro2::Span::call_site(),
                        format!("`{name}`: only its first form may have docs other than examples and shown"),
                    ));
                }
                sig.push(quote!(" / "));
            }
            sig.extend(f.sig.iter().cloned());
        }
        let examples = group.iter().flat_map(|f| &f.attr.examples);
        let shown = group.iter().flat_map(|f| &f.attr.shown);
        let (desc, detail) = (&first.desc, &first.detail);
        let aliases = first.attr.aliases.as_ref().map_or(quote!(&[]), |e| quote!(&#e));
        let see = first.attr.see.as_ref().map_or(quote!(&[]), |e| quote!(&#e));
        let fields = first.attr.fields.as_ref().map_or(quote!(&[]), |e| quote!(&#e));
        let (mutates, pretty) = (first.attr.mutates, first.attr.pretty);
        let flags = first.attr.flags.as_ref().map_or(quote!(None), |f| {
            let (c, n, m) = (f.iter().map(|f| f.0), f.iter().map(|f| &f.1), f.iter().map(|f| &f.2));
            quote!(Some(&[#((#c, #n, #m)),*]))
        });
        let command = first.attr.command.as_deref().unwrap_or("");
        docs.push(quote! {
            crate::modules::Doc {
                name: #name,
                aliases: #aliases,
                sig: {
                    const P: &[&str] = &[#(#sig),*];
                    const B: [u8; crate::signature::concat_len(P)] = crate::signature::concat(P);
                    match ::core::str::from_utf8(&B) {
                        Ok(s) => s,
                        Err(_) => panic!("sig is not UTF-8"),
                    }
                },
                desc: #desc,
                detail: #detail,
                examples: &[#(#examples),*],
                see: #see,
                shown: &[#(#shown),*],
                mutates: #mutates,
                pretty: #pretty,
                fields: #fields,
                flags: #flags,
                command: #command,
            }
        });
        let attempts = group.iter().map(|f| &f.attempt);
        arms.push(quote!(#name => { #(#attempts)* }));
    }
    items.push(syn::parse_quote! {
        pub const FNS: &[crate::modules::Doc] = &[#(#docs),*];
    });
    items.push(syn::parse_quote! {
        #[allow(unused_variables, irrefutable_let_patterns)]
        pub fn call(zil_it: &mut crate::interp::Interp, zil_name: &'static str, zil_args: &[crate::value::Value], zil_span: &crate::lexer::Span) -> crate::modules::Call {
            match zil_name {
                #(#arms)*
                _ => {}
            }
            Err(crate::modules::Fail::BadArgs)
        }
    });
    Ok(quote!(#m))
}

/// `[("r", "recursive", "meaning"), ...]`: each flag's letter, name and what it does.
fn flags(arr: ExprArray) -> syn::Result<Vec<(char, String, String)>> {
    let str = |e: &Expr| match e {
        Expr::Lit(syn::ExprLit { lit: Lit::Str(s), .. }) => Ok(s.value()),
        _ => Err(Error::new_spanned(e, "expected a string")),
    };
    arr.elems
        .iter()
        .map(|e| match e {
            Expr::Tuple(t) if t.elems.len() == 3 => {
                let letter = str(&t.elems[0])?;
                let mut cs = letter.chars();
                match (cs.next(), cs.next()) {
                    (Some(c), None) => Ok((c, str(&t.elems[1])?, str(&t.elems[2])?)),
                    _ => Err(Error::new_spanned(&t.elems[0], "a flag is one letter")),
                }
            }
            _ => Err(Error::new_spanned(e, "expected (\"letter\", \"name\", \"meaning\")")),
        })
        .collect()
}

fn form(f: &mut ItemFn, attr: &syn::Attribute) -> syn::Result<Form> {
    let mut a = Attr::default();
    if !matches!(attr.meta, Meta::Path(_)) {
        attr.parse_nested_meta(|meta| {
            let key = meta.path.get_ident().map(Ident::to_string).unwrap_or_default();
            match key.as_str() {
                "mutates" => a.mutates = true,
                "pretty" => a.pretty = true,
                "name" => a.name = Some(meta.value()?.parse::<syn::LitStr>()?.value()),
                "aliases" => a.aliases = Some(meta.value()?.parse()?),
                "flags" => a.flags = Some(flags(meta.value()?.parse()?)?),
                "command" => a.command = Some(meta.value()?.parse::<syn::LitStr>()?.value()),
                "examples" => a.examples = meta.value()?.parse::<ExprArray>()?.elems.into_iter().collect(),
                "shown" => a.shown = meta.value()?.parse::<ExprArray>()?.elems.into_iter().collect(),
                "see" => a.see = Some(meta.value()?.parse()?),
                "fields" => a.fields = Some(meta.value()?.parse()?),
                _ => return Err(meta.error("expected name, aliases, examples, see, shown, fields, flags, command, mutates or pretty")),
            }
            Ok(())
        })?;
    }
    let ident = f.sig.ident.clone();
    let name = a.name.clone().unwrap_or_else(|| ident.unraw().to_string());
    let (desc, detail) = docs(&f.attrs);

    let mut sig = vec![lit(format!("{name}("))];
    let (mut convs, mut vars, mut call_args) = (vec![], vec![], vec![]);
    let (mut min, mut optional, mut rest, mut takes_flags) = (0usize, false, false, false);
    for input in f.sig.inputs.iter_mut() {
        let FnArg::Typed(p) = input else { return Err(Error::new_spanned(input, "builtins take no self")) };
        let ty = &*p.ty;
        if is_ref_to(ty, "Interp", true) {
            call_args.push(quote!(zil_it));
            continue;
        }
        if is_ref_to(ty, "Span", false) {
            call_args.push(quote!(zil_span));
            continue;
        }
        if matches!(ty, Type::Path(p) if p.path.is_ident("Flags")) {
            takes_flags = true;
            call_args.push(quote!(crate::signature::Flags(zil_flags)));
            continue;
        }
        let Pat::Ident(pi) = &*p.pat else { return Err(Error::new_spanned(&p.pat, "builtin params need a plain name")) };
        let pname = pi.ident.unraw().to_string();
        let default = match p.attrs.iter().position(|a| a.path().is_ident("default")) {
            Some(i) => Some(p.attrs.remove(i).parse_args::<Expr>()?),
            None => None,
        };
        let i = vars.len();
        if !vars.is_empty() {
            sig.push(lit(", "));
        }
        let var = format_ident!("zil_a{i}");
        let ty = &*p.ty;
        if rest {
            return Err(Error::new_spanned(&p.pat, "a Rest param comes last"));
        }
        if let Some(inner) = generic(ty, "Rest") {
            rest = true;
            sig.push(lit(format!("*{pname}: ")));
            types(inner, &mut sig);
            convs.push(quote!(crate::signature::rest::<#inner>(zil_args.get(#i..).unwrap_or(&[]))));
        } else if let Some(inner) = generic(ty, "Option") {
            if default.is_some() {
                return Err(Error::new_spanned(ty, "an Option param already defaults to nil, so it takes no #[default]"));
            }
            optional = true;
            sig.push(lit(format!("{pname}?: ")));
            types(inner, &mut sig);
            convs.push(quote!(crate::signature::opt::<#inner>(zil_args.get(#i))));
        } else if let Some(d) = default {
            optional = true;
            sig.push(lit(format!("{pname}: ")));
            types(ty, &mut sig);
            sig.push(lit(format!(" = {}", zil_literal(&d)?)));
            convs.push(quote!(crate::signature::or::<#ty>(zil_args.get(#i), #d)));
        } else {
            if optional {
                return Err(Error::new_spanned(&p.pat, "required params come before optional ones"));
            }
            min += 1;
            sig.push(lit(format!("{pname}: ")));
            types(ty, &mut sig);
            convs.push(quote!(<#ty as crate::signature::Arg<'_>>::of(&zil_args[#i])));
        }
        vars.push(var.clone());
        call_args.push(quote!(#var));
    }
    if a.flags.is_some() != a.command.is_some() {
        return Err(Error::new_spanned(&f.sig.ident, "#[builtin(flags = ...)] and #[builtin(command = \"...\")] go together"));
    }
    match (&a.flags, takes_flags) {
        (Some(f), true) if !f.is_empty() => {
            let letters: String = f.iter().map(|f| f.0).collect();
            sig.push(lit(format!("{}-{letters}", if vars.is_empty() { "" } else { ", " })))
        }
        (Some(_), true) => {}
        (None, true) => return Err(Error::new_spanned(&f.sig.ident, "a Flags param needs #[builtin(flags = [...])]")),
        (Some(_), false) => return Err(Error::new_spanned(&f.sig.ident, "#[builtin(flags = ...)] needs a Flags param")),
        (None, false) => {}
    }
    sig.push(lit(") -> "));
    match &f.sig.output {
        ReturnType::Default => sig.push(lit("nil")),
        ReturnType::Type(_, ty) => returns(ty, &mut sig),
    }
    let max = if rest { usize::MAX } else { vars.len() };
    // A command's trailing flags map comes off before the arguments are matched.
    let strip = takes_flags.then(|| quote!(let (zil_args, zil_flags) = crate::modules::flags(zil_name, zil_args)?;));
    let attempt = quote! {
        {
            #strip
            if (#min..=#max).contains(&zil_args.len()) {
                if let (#(Some(#vars),)*) = (#(#convs,)*) {
                    return crate::signature::Ret::ret(#ident(#(#call_args),*));
                }
            }
        }
    };
    Ok(Form { name, desc, detail, attr: a, sig, attempt })
}

fn lit(s: impl AsRef<str>) -> Tokens {
    let s = s.as_ref();
    quote!(#s)
}

/// The desc (first paragraph of the `///` comment) and the detail (the rest), each joined into one line.
fn docs(attrs: &[syn::Attribute]) -> (String, String) {
    let lines: Vec<String> = attrs
        .iter()
        .filter_map(|a| match &a.meta {
            Meta::NameValue(nv) if nv.path.is_ident("doc") => match &nv.value {
                Expr::Lit(syn::ExprLit { lit: Lit::Str(s), .. }) => Some(s.value().trim().to_string()),
                _ => None,
            },
            _ => None,
        })
        .collect();
    let mut paras: Vec<String> = vec![];
    let mut fresh = true;
    for l in lines {
        match (l.is_empty(), fresh) {
            (true, _) => fresh = true,
            (false, true) => {
                paras.push(l);
                fresh = false;
            }
            (false, false) => {
                let last = paras.last_mut().unwrap();
                last.push(' ');
                last.push_str(&l);
            }
        }
    }
    let mut it = paras.into_iter();
    (it.next().unwrap_or_default(), it.collect::<Vec<_>>().join(" "))
}

/// A default as zil source: a literal, or a negated number. Strings must not be raw, since `r"..."` is a regex in zil.
fn zil_literal(e: &Expr) -> syn::Result<String> {
    match e {
        Expr::Lit(l) => match &l.lit {
            Lit::Str(s) if s.token().to_string().starts_with('r') => Err(Error::new_spanned(e, "a raw string reads as a regex in zil")),
            Lit::Str(_) | Lit::Int(_) | Lit::Float(_) | Lit::Bool(_) => Ok(quote!(#l).to_string()),
            _ => Err(Error::new_spanned(e, "defaults are str, int, float or bool literals")),
        },
        Expr::Unary(u) if matches!(u.op, UnOp::Neg(_)) && matches!(&*u.expr, Expr::Lit(_)) => Ok(format!("-{}", zil_literal(&u.expr)?)),
        _ => Err(Error::new_spanned(e, "defaults are literals")),
    }
}

/// A param type's sig text: `Or<A, B>` (or `Or3` to `Or5`) as `A|B`, anything else by its `Arg::TYPE`.
fn types(ty: &Type, out: &mut Vec<Tokens>) {
    if let Some(parts) = either(ty) {
        for (i, t) in parts.into_iter().enumerate() {
            if i > 0 {
                out.push(lit("|"));
            }
            types(t, out);
        }
    } else {
        out.push(quote!(<#ty as crate::signature::Arg>::TYPE));
    }
}

fn returns(ty: &Type, out: &mut Vec<Tokens>) {
    if let Some(ok) = generics(ty, "Result").and_then(|g| g.first().copied()) {
        returns(ok, out);
    } else if let Some(inner) = generic(ty, "Option") {
        returns(inner, out);
        out.push(lit("|nil"));
    } else if let Some(inner) = generic(ty, "As") {
        returns(inner, out);
    } else if let Some(parts) = either(ty) {
        for (i, t) in parts.into_iter().enumerate() {
            if i > 0 {
                out.push(lit("|"));
            }
            returns(t, out);
        }
    } else {
        out.push(quote!(<#ty as crate::signature::Ret>::TYPE));
    }
}

/// The choices of an `Or`, `Or3`, `Or4` or `Or5`.
fn either(ty: &Type) -> Option<Vec<&Type>> {
    ["Or", "Or3", "Or4", "Or5"].iter().find_map(|n| generics(ty, n))
}

/// `ty`'s type arguments if it's `name<...>`.
fn generics<'a>(ty: &'a Type, name: &str) -> Option<Vec<&'a Type>> {
    let Type::Path(p) = ty else { return None };
    let seg = p.path.segments.last()?;
    let PathArguments::AngleBracketed(args) = &seg.arguments else { return None };
    (seg.ident == name).then(|| args.args.iter().filter_map(|a| if let GenericArgument::Type(t) = a { Some(t) } else { None }).collect())
}

fn generic<'a>(ty: &'a Type, name: &str) -> Option<&'a Type> {
    generics(ty, name).and_then(|g| g.first().copied())
}

/// Whether `ty` is `&name` (or `&mut name`).
fn is_ref_to(ty: &Type, name: &str, mutable: bool) -> bool {
    let Type::Reference(r) = ty else { return false };
    let Type::Path(p) = &*r.elem else { return false };
    r.mutability.is_some() == mutable && p.path.segments.last().is_some_and(|s| s.ident == name)
}
