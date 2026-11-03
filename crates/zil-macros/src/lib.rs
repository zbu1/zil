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
/// - Params after a `Rest` (or marked `#[kw]`) are keyword-only: `f(x, name: value)`, read from a trailing map and shown after
///   `*` in the sig. `#[flag('r')]` makes a `bool` (or `Option`, set to `true`) one a command flag (`rm -rf`, `-r` is `recursive`), and `command = "rm -rf build"` is
///   the command-line example. A `///` on any param is its description in help.
/// - `aliases = ["cat"]` are other names for it, sharing its help page.
/// - `#[builtin(name = "range", aliases = [...], examples = [...], see = [...], shown = [...], tags = [...], fields = [...], command = "...", mutates, pretty)]`; forms
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
    command: Option<String>,
    tags: Option<Expr>,
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
    /// Command flags: letter, name and description.
    flags: Vec<(char, String, String)>,
    /// Described params: name and description.
    params: Vec<(String, String)>,
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
                    || !f.flags.is_empty()
                    || !f.params.is_empty()
                    || f.attr.command.is_some()
                    || f.attr.tags.is_some()
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
        let flags = if first.attr.command.is_some() {
            let f = &first.flags;
            let (c, n, m) = (f.iter().map(|f| f.0), f.iter().map(|f| &f.1), f.iter().map(|f| &f.2));
            quote!(Some(&[#((#c, #n, #m)),*]))
        } else {
            quote!(None)
        };
        let params = first.params.iter().map(|p| {
            let (n, d) = (&p.0, &p.1);
            quote!((#n, #d))
        });
        let command = first.attr.command.as_deref().unwrap_or("");
        let tags = first.attr.tags.as_ref().map_or(quote!(&[]), |e| quote!(&#e));
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
                params: &[#(#params),*],
                command: #command,
                tags: #tags,
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
                "command" => a.command = Some(meta.value()?.parse::<syn::LitStr>()?.value()),
                "examples" => a.examples = meta.value()?.parse::<ExprArray>()?.elems.into_iter().collect(),
                "shown" => a.shown = meta.value()?.parse::<ExprArray>()?.elems.into_iter().collect(),
                "tags" => a.tags = Some(meta.value()?.parse()?),
                "see" => a.see = Some(meta.value()?.parse()?),
                "fields" => a.fields = Some(meta.value()?.parse()?),
                _ => return Err(meta.error("expected name, aliases, examples, see, shown, fields, command, tags, mutates or pretty")),
            }
            Ok(())
        })?;
    }
    let ident = f.sig.ident.clone();
    let name = a.name.clone().unwrap_or_else(|| ident.unraw().to_string());
    let (desc, detail) = docs(&f.attrs);

    let mut sig = vec![lit(format!("{name}("))];
    let (mut convs, mut vars, mut call_args) = (vec![], vec![], vec![]);
    let (mut min, mut pos, mut optional, mut rest) = (0usize, 0usize, false, false);
    let (mut kw_names, mut flags, mut params) = (vec![], vec![], vec![]);
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
        let Pat::Ident(pi) = &*p.pat else { return Err(Error::new_spanned(&p.pat, "builtin params need a plain name")) };
        let pname = pi.ident.unraw().to_string();
        let default = match p.attrs.iter().position(|a| a.path().is_ident("default")) {
            Some(i) => Some(p.attrs.remove(i).parse_args::<Expr>()?),
            None => None,
        };
        let letter = match p.attrs.iter().position(|a| a.path().is_ident("flag")) {
            Some(i) => Some(p.attrs.remove(i).parse_args::<syn::LitChar>()?.value()),
            None => None,
        };
        let marked = match p.attrs.iter().position(|a| a.path().is_ident("kw")) {
            Some(i) => {
                p.attrs.remove(i);
                true
            }
            None => false,
        };
        let (desc, _) = docs(&p.attrs);
        p.attrs.retain(|a| !a.path().is_ident("doc"));
        let keyword = rest || marked || letter.is_some();
        let is_bool = matches!(&*p.ty, Type::Path(t) if t.path.is_ident("bool"));
        let default = match (&default, letter) {
            (None, Some(_)) if is_bool => Some(syn::parse_quote!(false)),
            _ => default,
        };
        if let Some(c) = letter {
            if !is_bool && !matches!(&*p.ty, Type::Path(t) if t.path.segments.last().is_some_and(|s| s.ident == "Option")) {
                return Err(Error::new_spanned(&p.ty, "a #[flag] param is a bool or an Option"));
            }
            flags.push((c, pname.clone(), desc.clone()));
        } else if !desc.is_empty() {
            params.push((pname.clone(), desc));
        }
        let var = format_ident!("zil_a{}", vars.len());
        let ty = &*p.ty;
        // Where this param's value comes from: the next positional argument, or the keyword map.
        let at = if keyword {
            let k = kw_names.len();
            kw_names.push(pname.clone());
            quote!(zil_kw.get(#k))
        } else {
            let i = pos;
            quote!(zil_args.get(#i))
        };
        let sep = if sig.len() > 1 { ", " } else { "" };
        if keyword && kw_names.len() == 1 && !rest {
            sig.push(lit(format!("{sep}*")));
        }
        let sep = if sig.len() > 1 { ", " } else { "" };
        if !keyword && generic(ty, "Rest").is_some() {
            let inner = generic(ty, "Rest").expect("checked");
            rest = true;
            sig.push(lit(format!("{sep}*{pname}: ")));
            types(inner, &mut sig);
            convs.push(quote!(crate::signature::rest::<#inner>(zil_args.get(#pos..).unwrap_or(&[]))));
        } else if let Some(inner) = generic(ty, "Option") {
            if default.is_some() {
                return Err(Error::new_spanned(ty, "an Option param already defaults to nil, so it takes no #[default]"));
            }
            if !keyword {
                optional = true;
            }
            sig.push(lit(format!("{sep}{pname}?: ")));
            types(inner, &mut sig);
            convs.push(quote!(crate::signature::opt::<#inner>(#at)));
        } else if let Some(d) = default {
            if !keyword {
                optional = true;
            }
            sig.push(lit(format!("{sep}{pname}: ")));
            types(ty, &mut sig);
            sig.push(lit(format!(" = {}", zil_literal(&d)?)));
            convs.push(quote!(crate::signature::or::<#ty>(#at, #d)));
        } else {
            if optional && !keyword {
                return Err(Error::new_spanned(&p.pat, "required params come before optional ones"));
            }
            if !keyword {
                min += 1;
            }
            sig.push(lit(format!("{sep}{pname}: ")));
            types(ty, &mut sig);
            convs.push(quote!(#at.and_then(<#ty as crate::signature::Arg<'_>>::of)));
        }
        if !keyword {
            pos += 1;
        }
        vars.push(var.clone());
        call_args.push(quote!(#var));
    }
    if a.command.is_none() && !flags.is_empty() {
        return Err(Error::new_spanned(&f.sig.ident, "#[flag] params need #[builtin(command = \"...\")]"));
    }
    sig.push(lit(") -> "));
    match &f.sig.output {
        ReturnType::Default => sig.push(lit("nil")),
        ReturnType::Type(_, ty) => returns(ty, &mut sig),
    }
    let max = if rest { usize::MAX } else { pos };
    let body = quote! {
        if (#min..=#max).contains(&zil_args.len()) {
            if let (#(Some(#vars),)*) = (#(#convs,)*) {
                return crate::signature::Ret::ret(#ident(#(#call_args),*));
            }
        }
    };
    // A form with keyword params takes their trailing map off before the arguments are matched.
    let attempt = if kw_names.is_empty() {
        quote!({ #body })
    } else {
        quote!({
            if let Some((zil_args, zil_kw)) = crate::modules::kw(zil_args, &[#(#kw_names),*]) {
                #body
            }
        })
    };
    Ok(Form { name, desc, detail, attr: a, sig, attempt, flags, params })
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
