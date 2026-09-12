//! `#[derive(PlutusCodec)]` — structs become integer-keyed maps, enums
//! become the same map with the variant tag at key `0`.
//!
//! The macro exists for one reason: the vocabulary is large and the failure
//! mode of a hand-written impl is a **silently reused field id**, which the
//! golden corpus then freezes forever. Everything the macro can refuse at
//! compile time, it refuses.
//!
//! ```ignore
//! #[derive(PlutusCodec)]
//! struct Grant {
//!     #[plutus(id = 0)] mode: Mode,                     // required
//!     #[plutus(id = 2)] fuel_cost: Option<u64>,         // absent when None
//!     #[plutus(id = 3, default)] grants: Vec<Grant>,    // absent when empty
//!     #[plutus(id = 4, default = 300)] depth: u64,      // absent when 300
//!     #[plutus(unknown)] unknown: UnknownFields,        // REQUIRED
//! }
//!
//! #[derive(PlutusCodec)]
//! enum Mode {
//!     // Variant field ids start at 1 — key 0 is the tag.
//!     #[plutus(tag = 0)] Guaranteed {
//!         #[plutus(id = 1)] per_unit: u64,
//!         #[plutus(unknown)] unknown: UnknownFields,
//!     },
//!     #[plutus(unknown)] Unknown { tag: i64, fields: UnknownFields },
//! }
//! ```
//!
//! **Unit variants carry no unknown block**, so a field added to one by a
//! future writer is dropped when an older reader re-encodes. The format's
//! answer is to add a new tag rather than fields to a unit variant.

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{
    parse_macro_input, spanned::Spanned, Data, DeriveInput, Expr, Field, Fields, Ident, Lit, Type,
    Variant,
};

#[proc_macro_derive(PlutusCodec, attributes(plutus))]
pub fn derive_plutus_codec(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    match expand(&input) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.to_compile_error().into(),
    }
}

fn expand(input: &DeriveInput) -> syn::Result<TokenStream2> {
    let name = &input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    let (to_data, from_data) = match &input.data {
        Data::Struct(data) => struct_body(name, &data.fields)?,
        Data::Enum(data) => enum_body(name, &data.variants)?,
        Data::Union(_) => {
            return Err(syn::Error::new(
                input.span(),
                "PlutusCodec cannot be derived for a union",
            ))
        }
    };

    Ok(quote! {
        impl #impl_generics ::action_definitions::codec::PlutusCodec for #name #ty_generics
            #where_clause
        {
            fn to_data(&self) -> ::pallas_primitives::PlutusData {
                #to_data
            }

            fn from_data(
                data: &::pallas_primitives::PlutusData,
            ) -> ::core::result::Result<Self, ::action_definitions::codec::DecodeError> {
                #from_data
            }
        }
    })
}

// ── field model ────────────────────────────────────────────────────────────

/// How one field is written and read.
enum FieldKind {
    /// No default: absent on decode is `MissingField`.
    Required,
    /// `Option<T>`: `None` is absence, never an explicit null.
    Optional,
    /// Absent when equal to `Default::default()`.
    DefaultImpl,
    /// Absent when equal to the given expression.
    DefaultExpr(Expr),
}

struct CodecField<'a> {
    ident: &'a Ident,
    id: i64,
    kind: FieldKind,
}

/// The `#[plutus(unknown)]` member, plus the coded fields.
struct FieldSet<'a> {
    coded: Vec<CodecField<'a>>,
    unknown: Option<&'a Ident>,
}

fn parse_fields<'a>(fields: &'a Fields, in_variant: bool) -> syn::Result<FieldSet<'a>> {
    let named = match fields {
        Fields::Named(named) => &named.named,
        Fields::Unit => {
            return Ok(FieldSet {
                coded: Vec::new(),
                unknown: None,
            })
        }
        Fields::Unnamed(unnamed) => {
            return Err(syn::Error::new(
                unnamed.span(),
                "PlutusCodec needs named fields — a tuple struct has no field ids to assign",
            ))
        }
    };

    let mut coded: Vec<CodecField<'a>> = Vec::new();
    let mut unknown = None;

    for field in named {
        let ident = field.ident.as_ref().expect("named");
        let attr = parse_field_attr(field)?;
        match attr {
            FieldAttr::Unknown => {
                if unknown.is_some() {
                    return Err(syn::Error::new(
                        field.span(),
                        "only one #[plutus(unknown)] field per type",
                    ));
                }
                unknown = Some(ident);
            }
            FieldAttr::Coded { id, kind } => {
                if is_byte_vec(&field.ty) {
                    return Err(syn::Error::new(
                        field.span(),
                        "use `Bytes` rather than `Vec<u8>` — `Vec<T>` encodes as a \
                         PlutusData array, so a `Vec<u8>` field would ship as a list \
                         of small integers instead of a bytestring",
                    ));
                }
                if in_variant && id == 0 {
                    return Err(syn::Error::new(
                        field.span(),
                        "variant field ids start at 1 — key 0 is the enum tag \
                         (ACTION_DEFINITION_SCHEMA.md §2.3). An id of 0 here would \
                         overwrite the tag on encode and be read back as the tag.",
                    ));
                }
                if let Some(prior) = coded.iter().find(|f| f.id == id) {
                    return Err(syn::Error::new(
                        field.span(),
                        format!(
                            "field id {id} is already used by `{}` — ids are assigned once \
                             and never reused (ACTION_DEFINITION_SCHEMA.md §2.2)",
                            prior.ident
                        ),
                    ));
                }
                coded.push(CodecField { ident, id, kind });
            }
            FieldAttr::None => {
                return Err(syn::Error::new(
                    field.span(),
                    "every field needs #[plutus(id = N)] or #[plutus(unknown)] — an \
                     unannotated field would be silently dropped on encode",
                ));
            }
        }
    }

    Ok(FieldSet { coded, unknown })
}

enum FieldAttr {
    None,
    Unknown,
    Coded { id: i64, kind: FieldKind },
}

fn parse_field_attr(field: &Field) -> syn::Result<FieldAttr> {
    let Some(attr) = field.attrs.iter().find(|a| a.path().is_ident("plutus")) else {
        return Ok(FieldAttr::None);
    };

    let mut id: Option<i64> = None;
    let mut is_unknown = false;
    let mut default: Option<Option<Expr>> = None;

    attr.parse_nested_meta(|meta| {
        if meta.path.is_ident("unknown") {
            is_unknown = true;
            return Ok(());
        }
        if meta.path.is_ident("id") {
            let value = meta.value()?;
            let lit: Lit = value.parse()?;
            let Lit::Int(int) = lit else {
                return Err(meta.error("id must be an integer literal"));
            };
            id = Some(int.base10_parse()?);
            return Ok(());
        }
        if meta.path.is_ident("default") {
            // Either bare `default` or `default = <expr>`.
            if meta.input.peek(syn::Token![=]) {
                let value = meta.value()?;
                default = Some(Some(value.parse()?));
            } else {
                default = Some(None);
            }
            return Ok(());
        }
        Err(meta.error("unknown #[plutus(...)] option; expected id, default or unknown"))
    })?;

    if is_unknown {
        if id.is_some() || default.is_some() {
            return Err(syn::Error::new(
                attr.span(),
                "#[plutus(unknown)] takes no other options",
            ));
        }
        return Ok(FieldAttr::Unknown);
    }

    let Some(id) = id else {
        return Ok(FieldAttr::None);
    };

    let kind = match default {
        Some(Some(expr)) => FieldKind::DefaultExpr(expr),
        Some(None) => FieldKind::DefaultImpl,
        None if is_option(&field.ty) => FieldKind::Optional,
        None => FieldKind::Required,
    };

    Ok(FieldAttr::Coded { id, kind })
}

/// `Option<T>` is the one type whose shape decides how it is *written*,
/// because absence is its whole meaning. Everything else is driven by the
/// attribute.
fn is_option(ty: &Type) -> bool {
    last_segment_is(ty, "Option")
}

/// `Vec<u8>` is refused outright — see the error text at the call site.
fn is_byte_vec(ty: &Type) -> bool {
    let Type::Path(path) = ty else { return false };
    let Some(segment) = path.path.segments.last() else {
        return false;
    };
    if segment.ident != "Vec" {
        return false;
    }
    let syn::PathArguments::AngleBracketed(args) = &segment.arguments else {
        return false;
    };
    args.args
        .iter()
        .any(|arg| matches!(arg, syn::GenericArgument::Type(inner) if last_segment_is(inner, "u8")))
}

fn last_segment_is(ty: &Type, name: &str) -> bool {
    let Type::Path(path) = ty else { return false };
    path.path
        .segments
        .last()
        .is_some_and(|segment| segment.ident == name)
}

// ── struct ─────────────────────────────────────────────────────────────────

fn struct_body(name: &Ident, fields: &Fields) -> syn::Result<(TokenStream2, TokenStream2)> {
    let set = parse_fields(fields, false)?;
    let Some(unknown) = set.unknown else {
        return Err(syn::Error::new(
            name.span(),
            "a PlutusCodec struct needs a #[plutus(unknown)] UnknownFields field — \
             without it an older build silently deletes fields it does not know \
             (ACTION_DEFINITION_SCHEMA.md §2.2)",
        ));
    };

    let writes: Vec<TokenStream2> = set
        .coded
        .iter()
        .map(|f| {
            let ident = f.ident;
            write_field(f, quote!(&self.#ident))
        })
        .collect();

    let ids = set.coded.iter().map(|f| f.id);
    let reads: Vec<TokenStream2> = set.coded.iter().map(read_field).collect();
    let names: Vec<&Ident> = set.coded.iter().map(|f| f.ident).collect();

    let to_data = quote! {
        let mut writer = ::action_definitions::codec::MapWriter::new();
        #(#writes)*
        writer.unknown(&self.#unknown);
        writer.finish()
    };

    let from_data = quote! {
        const KNOWN: &[i64] = &[#(#ids),*];
        let reader = ::action_definitions::codec::MapReader::new(data, KNOWN)?;
        ::core::result::Result::Ok(Self {
            #(#names: #reads,)*
            #unknown: reader.into_unknown(),
        })
    };

    Ok((to_data, from_data))
}

// ── enum ───────────────────────────────────────────────────────────────────

fn enum_body(
    name: &Ident,
    variants: &syn::punctuated::Punctuated<Variant, syn::Token![,]>,
) -> syn::Result<(TokenStream2, TokenStream2)> {
    let mut write_arms: Vec<TokenStream2> = Vec::new();
    let mut read_arms: Vec<TokenStream2> = Vec::new();
    let mut seen_tags: Vec<(i64, Ident)> = Vec::new();
    let mut unknown_arm: Option<TokenStream2> = None;
    let mut unknown_read: Option<TokenStream2> = None;

    for variant in variants {
        let ident = &variant.ident;
        let attr = variant_attr(variant)?;

        match attr {
            VariantAttr::Unknown => {
                if unknown_arm.is_some() {
                    return Err(syn::Error::new(
                        variant.span(),
                        "only one #[plutus(unknown)] variant per enum",
                    ));
                }
                let (tag_field, fields_field) = unknown_variant_fields(variant)?;
                unknown_arm = Some(quote! {
                    Self::#ident { #tag_field, #fields_field } => {
                        let mut writer = ::action_definitions::codec::MapWriter::new();
                        writer.tag(*#tag_field);
                        writer.unknown(#fields_field);
                        writer.finish()
                    }
                });
                unknown_read = Some(quote! {
                    other => ::core::result::Result::Ok(Self::#ident {
                        #tag_field: other,
                        #fields_field: probe.into_unknown(),
                    })
                });
            }
            VariantAttr::Tagged(tag) => {
                if let Some((_, prior)) = seen_tags.iter().find(|(t, _)| *t == tag) {
                    return Err(syn::Error::new(
                        variant.span(),
                        format!(
                            "variant tag {tag} is already used by `{prior}` — tags are \
                             assigned once and never reused \
                             (ACTION_DEFINITION_SCHEMA.md §2.3)"
                        ),
                    ));
                }
                seen_tags.push((tag, ident.clone()));

                let set = parse_fields(&variant.fields, true)?;
                let is_unit = matches!(variant.fields, Fields::Unit);
                if !is_unit && set.unknown.is_none() {
                    return Err(syn::Error::new(
                        variant.span(),
                        "a variant with fields needs a #[plutus(unknown)] UnknownFields \
                         member, for the same reason a struct does",
                    ));
                }

                let names: Vec<&Ident> = set.coded.iter().map(|f| f.ident).collect();
                let writes: Vec<TokenStream2> = set
                    .coded
                    .iter()
                    .map(|f| {
                        let ident = f.ident;
                        write_field(f, quote!(#ident))
                    })
                    .collect();
                let reads: Vec<TokenStream2> = set.coded.iter().map(read_field).collect();
                let ids = set.coded.iter().map(|f| f.id);

                let (bind, write_unknown, read_unknown) = match set.unknown {
                    Some(u) => (
                        quote!({ #(#names,)* #u }),
                        quote!(writer.unknown(#u);),
                        quote!(#u: reader.into_unknown(),),
                    ),
                    None => (quote!(), quote!(), quote!()),
                };

                write_arms.push(quote! {
                    Self::#ident #bind => {
                        let mut writer = ::action_definitions::codec::MapWriter::new();
                        writer.tag(#tag);
                        #(#writes)*
                        #write_unknown
                        writer.finish()
                    }
                });

                read_arms.push(quote! {
                    #tag => {
                        const KNOWN: &[i64] = &[0, #(#ids),*];
                        let reader =
                            ::action_definitions::codec::MapReader::new(data, KNOWN)?;
                        ::core::result::Result::Ok(Self::#ident {
                            #(#names: #reads,)*
                            #read_unknown
                        })
                    }
                });
            }
        }
    }

    let Some(unknown_arm) = unknown_arm else {
        return Err(syn::Error::new(
            name.span(),
            "a PlutusCodec enum needs a #[plutus(unknown)] variant — without it a \
             definition using a newer variant is an ERROR on an older build instead \
             of being inert (ACTION_DEFINITION_SCHEMA.md §2.3)",
        ));
    };
    let unknown_read = unknown_read.expect("set with unknown_arm");

    let to_data = quote! {
        match self {
            #(#write_arms)*
            #unknown_arm
        }
    };

    let from_data = quote! {
        // Two passes over a small map: the tag decides which ids are known,
        // and everything outside that set is preserved as unknown.
        let probe = ::action_definitions::codec::MapReader::new(data, &[0])?;
        match probe.tag()? {
            #(#read_arms)*
            #unknown_read
        }
    };

    Ok((to_data, from_data))
}

enum VariantAttr {
    Unknown,
    Tagged(i64),
}

fn variant_attr(variant: &Variant) -> syn::Result<VariantAttr> {
    let Some(attr) = variant.attrs.iter().find(|a| a.path().is_ident("plutus")) else {
        return Err(syn::Error::new(
            variant.span(),
            "every variant needs #[plutus(tag = N)] or #[plutus(unknown)]",
        ));
    };

    let mut tag: Option<i64> = None;
    let mut is_unknown = false;

    attr.parse_nested_meta(|meta| {
        if meta.path.is_ident("unknown") {
            is_unknown = true;
            return Ok(());
        }
        if meta.path.is_ident("tag") {
            let lit: Lit = meta.value()?.parse()?;
            let Lit::Int(int) = lit else {
                return Err(meta.error("tag must be an integer literal"));
            };
            tag = Some(int.base10_parse()?);
            return Ok(());
        }
        Err(meta.error("unknown #[plutus(...)] option on a variant; expected tag or unknown"))
    })?;

    match (is_unknown, tag) {
        (true, None) => Ok(VariantAttr::Unknown),
        (false, Some(tag)) => Ok(VariantAttr::Tagged(tag)),
        (true, Some(_)) => Err(syn::Error::new(
            variant.span(),
            "#[plutus(unknown)] and #[plutus(tag)] are mutually exclusive",
        )),
        (false, None) => Err(syn::Error::new(
            variant.span(),
            "every variant needs #[plutus(tag = N)] or #[plutus(unknown)]",
        )),
    }
}

/// The catch-all variant must be `Unknown { tag: i64, fields: UnknownFields }`
/// in shape, whatever the two members are called.
fn unknown_variant_fields(variant: &Variant) -> syn::Result<(Ident, Ident)> {
    let Fields::Named(named) = &variant.fields else {
        return Err(syn::Error::new(
            variant.span(),
            "the #[plutus(unknown)] variant needs named fields: { tag, fields }",
        ));
    };
    let idents: Vec<Ident> = named
        .named
        .iter()
        .map(|f| f.ident.clone().expect("named"))
        .collect();
    if idents.len() != 2 {
        return Err(syn::Error::new(
            variant.span(),
            "the #[plutus(unknown)] variant needs exactly two fields: the tag and the \
             preserved fields",
        ));
    }
    Ok((idents[0].clone(), idents[1].clone()))
}

// ── field codegen ──────────────────────────────────────────────────────────

fn write_field(field: &CodecField<'_>, access: TokenStream2) -> TokenStream2 {
    let id = field.id;
    match &field.kind {
        FieldKind::Required => quote!(writer.field(#id, #access);),
        FieldKind::Optional => quote!(writer.opt(#id, #access);),
        FieldKind::DefaultImpl => {
            quote!(writer.with_default(#id, #access, &::core::default::Default::default());)
        }
        FieldKind::DefaultExpr(expr) => quote!(writer.with_default(#id, #access, &(#expr));),
    }
}

fn read_field(field: &CodecField<'_>) -> TokenStream2 {
    let id = field.id;
    match &field.kind {
        FieldKind::Required => quote!(reader.required(#id)?),
        FieldKind::Optional => quote!(reader.optional(#id)?),
        FieldKind::DefaultImpl => quote!(reader.or_default(#id)?),
        FieldKind::DefaultExpr(expr) => quote!(reader.or_else(#id, #expr)?),
    }
}

#[cfg(test)]
mod tests {
    //! What the macro **refuses**, tested by calling [`expand`] directly.
    //!
    //! These are compile errors in real use. Testing them here rather than
    //! with `trybuild` keeps the guarantee close to the rule and avoids
    //! pinning rustc's exact diagnostic text in a `.stderr` file that goes
    //! stale on every toolchain bump — the failure mode where a test that
    //! was protecting something quietly stops.

    use super::expand;

    fn error(source: &str) -> String {
        let input: syn::DeriveInput = syn::parse_str(source).expect("parses as Rust");
        match expand(&input) {
            Ok(_) => panic!("expected a compile error, but the macro accepted:\n{source}"),
            Err(err) => err.to_string(),
        }
    }

    fn accepts(source: &str) {
        let input: syn::DeriveInput = syn::parse_str(source).expect("parses as Rust");
        if let Err(err) = expand(&input) {
            panic!("expected acceptance, got `{err}` for:\n{source}");
        }
    }

    #[test]
    fn a_reused_field_id_is_refused() {
        let message = error(
            "struct S {
                #[plutus(id = 0)] a: u64,
                #[plutus(id = 0)] b: u64,
                #[plutus(unknown)] unknown: UnknownFields,
            }",
        );
        assert!(
            message.contains("field id 0 is already used by `a`"),
            "{message}"
        );
        assert!(message.contains("never reused"), "{message}");
    }

    #[test]
    fn a_reused_variant_tag_is_refused() {
        let message = error(
            "enum E {
                #[plutus(tag = 1)] A { #[plutus(unknown)] unknown: UnknownFields },
                #[plutus(tag = 1)] B { #[plutus(unknown)] unknown: UnknownFields },
                #[plutus(unknown)] Unknown { tag: i64, fields: UnknownFields },
            }",
        );
        assert!(
            message.contains("variant tag 1 is already used by `A`"),
            "{message}"
        );
    }

    /// The one that would have been frozen into the corpus: key 0 is the
    /// enum tag, so a variant field there overwrites it on encode and is
    /// read back as the tag.
    #[test]
    fn a_variant_field_at_id_zero_is_refused() {
        let message = error(
            "enum E {
                #[plutus(tag = 0)] A {
                    #[plutus(id = 0)] amount: u64,
                    #[plutus(unknown)] unknown: UnknownFields,
                },
                #[plutus(unknown)] Unknown { tag: i64, fields: UnknownFields },
            }",
        );
        assert!(
            message.contains("variant field ids start at 1"),
            "{message}"
        );
    }

    /// A struct's own ids DO start at 0 — there is no tag to collide with.
    #[test]
    fn a_struct_field_at_id_zero_is_fine() {
        accepts(
            "struct S {
                #[plutus(id = 0)] a: u64,
                #[plutus(unknown)] unknown: UnknownFields,
            }",
        );
    }

    #[test]
    fn a_type_without_an_unknown_member_is_refused() {
        let message = error("struct S { #[plutus(id = 0)] a: u64 }");
        assert!(message.contains("#[plutus(unknown)]"), "{message}");
        assert!(message.contains("silently deletes"), "{message}");
    }

    #[test]
    fn an_enum_without_an_unknown_variant_is_refused() {
        let message = error(
            "enum E {
                #[plutus(tag = 0)] A { #[plutus(unknown)] unknown: UnknownFields },
            }",
        );
        assert!(message.contains("#[plutus(unknown)] variant"), "{message}");
        assert!(message.contains("inert"), "{message}");
    }

    #[test]
    fn a_variant_with_fields_needs_its_own_unknown_block() {
        let message = error(
            "enum E {
                #[plutus(tag = 0)] A { #[plutus(id = 1)] amount: u64 },
                #[plutus(unknown)] Unknown { tag: i64, fields: UnknownFields },
            }",
        );
        assert!(message.contains("variant with fields needs"), "{message}");
    }

    /// A unit variant has nowhere to put one, and that is a documented
    /// trade: a field added to a unit variant later is dropped by an older
    /// reader, so the format's answer is a new tag instead.
    #[test]
    fn a_unit_variant_needs_no_unknown_block() {
        accepts(
            "enum E {
                #[plutus(tag = 0)] A,
                #[plutus(unknown)] Unknown { tag: i64, fields: UnknownFields },
            }",
        );
    }

    #[test]
    fn a_byte_vector_is_refused_in_favour_of_bytes() {
        let message = error(
            "struct S {
                #[plutus(id = 0)] blob: Vec<u8>,
                #[plutus(unknown)] unknown: UnknownFields,
            }",
        );
        assert!(
            message.contains("use `Bytes` rather than `Vec<u8>`"),
            "{message}"
        );
    }

    #[test]
    fn a_vector_of_anything_else_is_fine() {
        accepts(
            "struct S {
                #[plutus(id = 0, default)] items: Vec<Grant>,
                #[plutus(unknown)] unknown: UnknownFields,
            }",
        );
    }

    #[test]
    fn an_unannotated_field_is_refused_rather_than_silently_dropped() {
        let message = error(
            "struct S {
                #[plutus(id = 0)] a: u64,
                b: u64,
                #[plutus(unknown)] unknown: UnknownFields,
            }",
        );
        assert!(message.contains("every field needs"), "{message}");
    }

    #[test]
    fn a_variant_without_a_tag_is_refused() {
        let message = error(
            "enum E {
                A { #[plutus(unknown)] unknown: UnknownFields },
                #[plutus(unknown)] Unknown { tag: i64, fields: UnknownFields },
            }",
        );
        assert!(message.contains("every variant needs"), "{message}");
    }

    #[test]
    fn a_tuple_struct_is_refused_because_it_has_no_ids_to_assign() {
        let message = error("struct S(u64);");
        assert!(message.contains("named fields"), "{message}");
    }
}
