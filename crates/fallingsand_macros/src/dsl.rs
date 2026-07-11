use proc_macro2::Span;
use syn::parse::{Parse, ParseStream};
use syn::{Expr, ExprLit, Ident, Lit, Token, braced, bracketed};

pub struct Header {
    pub ember_colors: Vec<[u8; 4]>,
    pub material_files: Vec<String>,
    pub reactions_file: String,
}

impl Parse for Header {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut ember_colors = None;
        let mut material_files = None;
        let mut reactions_file = None;
        while !input.is_empty() {
            let key: Ident = input.parse()?;
            input.parse::<Token![:]>()?;
            let duplicate = match key.to_string().as_str() {
                "ember_colors" => ember_colors.is_some(),
                "materials" => material_files.is_some(),
                "reactions" => reactions_file.is_some(),
                _ => false,
            };
            if duplicate {
                return Err(syn::Error::new(key.span(), format!("duplicate `{key}`")));
            }
            match key.to_string().as_str() {
                "ember_colors" => {
                    let expr: Expr = input.parse()?;
                    ember_colors = Some(expr_colors(&expr, "content!", "ember_colors")?);
                }
                "materials" => {
                    let list;
                    bracketed!(list in input);
                    let mut files = Vec::new();
                    while !list.is_empty() {
                        files.push(list.parse::<syn::LitStr>()?.value());
                        if list.is_empty() {
                            break;
                        }
                        list.parse::<Token![,]>()?;
                    }
                    material_files = Some(files);
                }
                "reactions" => reactions_file = Some(input.parse::<syn::LitStr>()?.value()),
                other => {
                    return Err(syn::Error::new(
                        key.span(),
                        format!("unknown content! key `{other}`"),
                    ));
                }
            }
            if input.is_empty() {
                break;
            }
            input.parse::<Token![,]>()?;
        }
        let missing = |what: &str| syn::Error::new(Span::call_site(), format!("missing `{what}`"));
        Ok(Header {
            ember_colors: ember_colors.ok_or_else(|| missing("ember_colors"))?,
            material_files: material_files.ok_or_else(|| missing("materials"))?,
            reactions_file: reactions_file.ok_or_else(|| missing("reactions"))?,
        })
    }
}

pub struct MaterialAst {
    pub name: Ident,
    pub base: Option<Ident>,
    pub fields: Vec<(Ident, Expr)>,
}

struct MaterialFileAst {
    defs: Vec<MaterialAst>,
}

impl Parse for MaterialFileAst {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut defs = Vec::new();
        while !input.is_empty() {
            let name: Ident = input.parse()?;
            input.parse::<Token![=]>()?;
            let marker: Ident = input.parse()?;
            if marker != "Material" {
                return Err(syn::Error::new(marker.span(), "expected `Material { .. }`"));
            }
            let body;
            braced!(body in input);
            let mut base = None;
            let mut fields = Vec::new();
            while !body.is_empty() {
                if body.peek(Token![..]) {
                    body.parse::<Token![..]>()?;
                    base = Some(body.parse()?);
                    if !body.is_empty() {
                        return Err(syn::Error::new(
                            body.span(),
                            "`..BASE` must be the last entry",
                        ));
                    }
                    break;
                }
                let field: Ident = body.parse()?;
                body.parse::<Token![:]>()?;
                let value: Expr = body.parse()?;
                if matches!(value, Expr::Range(_)) {
                    return Err(syn::Error::new(field.span(), "missing `,` before `..`"));
                }
                fields.push((field, value));
                if body.is_empty() {
                    break;
                }
                body.parse::<Token![,]>()?;
            }
            defs.push(MaterialAst { name, base, fields });
            if input.peek(Token![,]) {
                input.parse::<Token![,]>()?;
            }
        }
        Ok(MaterialFileAst { defs })
    }
}

pub enum OperandAst {
    Material(Ident),
    Tag(Ident),
}

pub struct ReactionAst {
    pub a: OperandAst,
    pub b: OperandAst,
    pub a_becomes: Ident,
    pub b_becomes: Ident,
    pub rate: f32,
}

pub struct DecayAst {
    pub from: Ident,
    pub into: Ident,
    pub rate: f32,
}

struct ReactionFileAst {
    reactions: Vec<ReactionAst>,
    decays: Vec<DecayAst>,
}

fn parse_operand(input: ParseStream) -> syn::Result<OperandAst> {
    if input.peek(syn::token::Bracket) {
        let inner;
        bracketed!(inner in input);
        Ok(OperandAst::Tag(inner.parse()?))
    } else {
        Ok(OperandAst::Material(input.parse()?))
    }
}

fn parse_rate(input: ParseStream) -> syn::Result<f32> {
    let lit: Lit = input.parse()?;
    match lit {
        Lit::Float(lit) => lit.base10_parse(),
        Lit::Int(lit) => lit.base10_parse(),
        other => Err(syn::Error::new(other.span(), "expected a rate")),
    }
}

impl Parse for ReactionFileAst {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut reactions = Vec::new();
        let mut decays = Vec::new();
        while !input.is_empty() {
            let a = parse_operand(input)?;
            if input.peek(Token![=>]) {
                input.parse::<Token![=>]>()?;
                let into: Ident = input.parse()?;
                input.parse::<Token![@]>()?;
                let rate = parse_rate(input)?;
                input.parse::<Token![;]>()?;
                let OperandAst::Material(from) = a else {
                    return Err(fail(
                        "reactions: decay needs a specific material, not a tag",
                    ));
                };
                decays.push(DecayAst { from, into, rate });
                continue;
            }
            input.parse::<Token![+]>()?;
            let b = parse_operand(input)?;
            input.parse::<Token![=>]>()?;
            let a_becomes: Ident = input.parse()?;
            input.parse::<Token![+]>()?;
            let b_becomes: Ident = input.parse()?;
            input.parse::<Token![@]>()?;
            let rate = parse_rate(input)?;
            input.parse::<Token![;]>()?;
            reactions.push(ReactionAst {
                a,
                b,
                a_becomes,
                b_becomes,
                rate,
            });
        }
        Ok(ReactionFileAst { reactions, decays })
    }
}

pub struct Sources {
    pub materials: Vec<(String, Vec<MaterialAst>)>,
    pub reactions: Vec<ReactionAst>,
    pub decays: Vec<DecayAst>,
}

pub fn read_sources(header: &Header) -> syn::Result<Sources> {
    let manifest = std::env::var("CARGO_MANIFEST_DIR")
        .map_err(|_| fail("CARGO_MANIFEST_DIR is not set; build with cargo"))?;
    let root = std::path::Path::new(&manifest).join("content");

    let mut materials = Vec::new();
    for rel in &header.material_files {
        let file: MaterialFileAst = parse_file(&root, rel)?;
        materials.push((rel.clone(), file.defs));
    }
    let rules: ReactionFileAst = parse_file(&root, &header.reactions_file)?;
    Ok(Sources {
        materials,
        reactions: rules.reactions,
        decays: rules.decays,
    })
}

fn parse_file<T: Parse>(root: &std::path::Path, rel: &str) -> syn::Result<T> {
    let path = root.join(rel);
    let text = std::fs::read_to_string(&path)
        .map_err(|err| fail(format!("{rel}: cannot read {}: {err}", path.display())))?;
    let tokens: proc_macro2::TokenStream = text
        .parse()
        .map_err(|err: proc_macro2::LexError| fail(format!("{rel}: {err}")))?;
    syn::parse2(tokens).map_err(|err| {
        let at = err.span().start();
        fail(format!("{rel}:{}:{}: {err}", at.line, at.column + 1))
    })
}

pub fn fail(msg: impl std::fmt::Display) -> syn::Error {
    syn::Error::new(Span::call_site(), msg)
}

pub fn expr_f32(expr: &Expr, file: &str, context: &str) -> syn::Result<f32> {
    match expr {
        Expr::Lit(ExprLit {
            lit: Lit::Float(lit),
            ..
        }) => lit.base10_parse(),
        Expr::Lit(ExprLit {
            lit: Lit::Int(lit), ..
        }) => lit.base10_parse(),
        Expr::Unary(unary) if matches!(unary.op, syn::UnOp::Neg(_)) => Err(fail(format!(
            "{file}: {context}: negative values are not allowed"
        ))),
        _ => Err(fail(format!("{file}: {context}: expected a number"))),
    }
}

pub fn expr_bool(expr: &Expr, file: &str, context: &str) -> syn::Result<bool> {
    match expr {
        Expr::Lit(ExprLit {
            lit: Lit::Bool(lit),
            ..
        }) => Ok(lit.value),
        _ => Err(fail(format!("{file}: {context}: expected true or false"))),
    }
}

fn expr_ident(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Path(path) => path.path.get_ident().map(Ident::to_string),
        _ => None,
    }
}

pub fn expr_handle(expr: &Expr, file: &str, context: &str) -> syn::Result<String> {
    expr_ident(expr).ok_or_else(|| fail(format!("{file}: {context}: expected a material name")))
}

pub fn expr_phase(
    expr: &Expr,
    file: &str,
    context: &str,
) -> syn::Result<(String, Vec<(Ident, Expr)>)> {
    let expected = || {
        fail(format!(
            "{file}: {context}: expected a phase (Empty, Solid, Powder, Liquid, Gas), optionally with a field block"
        ))
    };
    match expr {
        Expr::Path(_) => Ok((expr_ident(expr).ok_or_else(expected)?, Vec::new())),
        Expr::Struct(block) => {
            let phase = block.path.get_ident().ok_or_else(expected)?.to_string();
            if block.rest.is_some() || block.dot2_token.is_some() {
                return Err(expected());
            }
            let fields = block
                .fields
                .iter()
                .map(|field| match &field.member {
                    syn::Member::Named(ident) => Ok((ident.clone(), field.expr.clone())),
                    syn::Member::Unnamed(_) => Err(expected()),
                })
                .collect::<syn::Result<Vec<_>>>()?;
            Ok((phase, fields))
        }
        _ => Err(expected()),
    }
}

pub fn expr_tags(expr: &Expr, file: &str, context: &str) -> syn::Result<Vec<String>> {
    let Expr::Array(array) = expr else {
        return Err(fail(format!("{file}: {context}: expected a tag list [..]")));
    };
    array
        .elems
        .iter()
        .map(|elem| {
            expr_ident(elem).ok_or_else(|| fail(format!("{file}: {context}: expected a tag name")))
        })
        .collect()
}

pub fn expr_colors(expr: &Expr, file: &str, context: &str) -> syn::Result<Vec<[u8; 4]>> {
    let Expr::Array(array) = expr else {
        return Err(fail(format!(
            "{file}: {context}: expected a color list [[r, g, b, a], ..]"
        )));
    };
    array
        .elems
        .iter()
        .map(|elem| {
            let Expr::Array(rgba) = elem else {
                return Err(fail(format!("{file}: {context}: expected [r, g, b, a]")));
            };
            if rgba.elems.len() != 4 {
                return Err(fail(format!(
                    "{file}: {context}: colors need exactly 4 components"
                )));
            }
            let mut out = [0u8; 4];
            for (slot, component) in out.iter_mut().zip(rgba.elems.iter()) {
                let Expr::Lit(ExprLit {
                    lit: Lit::Int(lit), ..
                }) = component
                else {
                    return Err(fail(format!("{file}: {context}: expected 0-255 integers")));
                };
                *slot = lit.base10_parse()?;
            }
            Ok(out)
        })
        .collect()
}
