use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, DeriveInput, Expr, Fields, Lit, LitStr, Type};

enum ULogType {
    Scalar {
        ulog_name: &'static str,
        size: usize,
    },
    Array {
        elem_ulog_name: &'static str,
        elem_size: usize,
        len: usize,
    },
}

impl ULogType {
    fn size(&self) -> usize {
        match self {
            Self::Scalar { size, .. } => *size,
            Self::Array { elem_size, len, .. } => elem_size * len,
        }
    }

    fn format_ulog(&self) -> String {
        match self {
            Self::Scalar { ulog_name, .. } => ulog_name.to_string(),
            Self::Array {
                elem_ulog_name,
                len,
                ..
            } => format!("{elem_ulog_name}[{len}]"),
        }
    }
}

struct FieldInfo {
    name: String,
    ty: ULogType,
}

#[proc_macro_derive(ULogMessage, attributes(uf_ulog))]
pub fn derive_ulog_message(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    expand_derive(&input)
        .unwrap_or_else(|err| err.to_compile_error())
        .into()
}

fn expand_derive(input: &DeriveInput) -> syn::Result<proc_macro2::TokenStream> {
    let ident = &input.ident;
    let fields = extract_named_fields(&input.data, ident)?;
    validate_timestamp_field(fields, ident)?;

    let field_infos: Vec<FieldInfo> = fields.iter().map(process_field).collect::<Result<_, _>>()?;

    let format_string = build_format_string(&field_infos);
    let size_terms: Vec<usize> = field_infos.iter().map(|fi| fi.ty.size()).collect();

    let message_name = extract_ulog_name(input)?;

    Ok(generate_impl(
        ident,
        &message_name,
        &format_string,
        &size_terms,
    ))
}

fn extract_ulog_name(input: &DeriveInput) -> syn::Result<LitStr> {
    let mut name = None;
    for attr in input.attrs.iter().filter(|attr| attr.path().is_ident("uf_ulog")) {
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("name") {
                if name.is_some() {
                    return Err(meta.error("duplicate `name` in `#[uf_ulog(...)]`"));
                }
                name = Some(meta.value()?.parse::<LitStr>()?);
                return Ok(());
            }
            Err(meta.error("unsupported uf_ulog attribute key, expected `name`"))
        })?;
    }

    Ok(name.unwrap_or_else(|| LitStr::new(&input.ident.to_string(), input.ident.span())))
}

fn extract_named_fields<'a>(
    data: &'a syn::Data,
    ident: &syn::Ident,
) -> syn::Result<&'a syn::punctuated::Punctuated<syn::Field, syn::token::Comma>> {
    match data {
        syn::Data::Struct(data_struct) => match &data_struct.fields {
            Fields::Named(fields) => Ok(&fields.named),
            _ => Err(syn::Error::new_spanned(
                ident,
                "ULogMessage requires a struct with named fields",
            )),
        },
        _ => Err(syn::Error::new_spanned(
            ident,
            "Only structs can derive ULogMessage",
        )),
    }
}

fn validate_timestamp_field(
    fields: &syn::punctuated::Punctuated<syn::Field, syn::token::Comma>,
    ident: &syn::Ident,
) -> syn::Result<()> {
    let timestamp_field = fields
        .iter()
        .find(|f| f.ident.as_ref().map_or(false, |id| id == "timestamp"))
        .ok_or_else(|| {
            syn::Error::new_spanned(ident, "ULog message must contain a `timestamp: u64` field")
        })?;

    if !is_u64_type(&timestamp_field.ty) {
        return Err(syn::Error::new_spanned(
            &timestamp_field.ty,
            "`timestamp` field must be `u64` for ULog compatibility",
        ));
    }

    Ok(())
}

fn process_field(field: &syn::Field) -> Result<FieldInfo, syn::Error> {
    let field_name = field
        .ident
        .as_ref()
        .ok_or_else(|| syn::Error::new_spanned(field, "expected named field"))?;
    let ty = type_spec(&field.ty)?;

    Ok(FieldInfo {
        name: field_name.to_string(),
        ty,
    })
}

fn build_format_string(field_infos: &[FieldInfo]) -> String {
    field_infos
        .iter()
        .map(|fi| format!("{} {};", fi.ty.format_ulog(), fi.name))
        .collect::<Vec<_>>()
        .join("")
}

fn generate_impl(
    ident: &syn::Ident,
    message_name: &LitStr,
    format_string: &str,
    size_terms: &[usize],
) -> proc_macro2::TokenStream {
    let format_lit = LitStr::new(format_string, ident.span());

    quote! {
        impl ::uf_ulog::ULogMessage for #ident {
            const NAME: &'static str = #message_name;
            const WIRE_SIZE: usize = 0 #(+ #size_terms)*;
            const FORMAT: &'static str = #format_lit;

            #[inline]
            fn encode(&self, buf: &mut [u8]) {
                let _ = buf;
            }
            #[inline]
            fn timestamp(&self) -> u64 {
                self.timestamp
            }
        }
    }
}

fn is_u64_type(ty: &Type) -> bool {
    primitive_type_name(ty).is_some_and(|ident| ident == "u64")
}

fn primitive_type_name(ty: &Type) -> Option<&syn::Ident> {
    match ty {
        Type::Path(type_path)
            if type_path.qself.is_none() && type_path.path.segments.len() == 1 =>
        {
            Some(&type_path.path.segments[0].ident)
        }
        _ => None,
    }
}

fn primitive_spec(ident: &syn::Ident) -> Option<(&'static str, usize)> {
    match ident.to_string().as_str() {
        "u8" => Some(("uint8_t", 1)),
        "i8" => Some(("int8_t", 1)),
        "u16" => Some(("uint16_t", 2)),
        "i16" => Some(("int16_t", 2)),
        "u32" => Some(("uint32_t", 4)),
        "i32" => Some(("int32_t", 4)),
        "u64" => Some(("uint64_t", 8)),
        "i64" => Some(("int64_t", 8)),
        "f32" => Some(("float", 4)),
        "f64" => Some(("double", 8)),
        "bool" => Some(("bool", 1)),
        _ => None,
    }
}

fn type_spec(ty: &Type) -> Result<ULogType, syn::Error> {
    if let Some(ident) = primitive_type_name(ty) {
        if let Some((ulog_name, size)) = primitive_spec(ident) {
            return Ok(ULogType::Scalar { ulog_name, size });
        }
    }

    match ty {
        Type::Array(array_ty) => {
            let elem_ident = primitive_type_name(&array_ty.elem).ok_or_else(|| {
                syn::Error::new_spanned(&array_ty.elem, "unsupported array element type")
            })?;
            let (elem_ulog_name, elem_size) = primitive_spec(elem_ident).ok_or_else(|| {
                syn::Error::new_spanned(&array_ty.elem, "unsupported array element type")
            })?;

            let len = extract_array_len(&array_ty.len)?;

            Ok(ULogType::Array {
                elem_ulog_name,
                elem_size,
                len,
            })
        }
        _ => Err(syn::Error::new_spanned(ty, "unsupported field type")),
    }
}

fn extract_array_len(len_expr: &syn::Expr) -> syn::Result<usize> {
    match len_expr {
        Expr::Lit(expr_lit) => match &expr_lit.lit {
            Lit::Int(int_lit) => int_lit.base10_parse::<usize>(),
            _ => Err(syn::Error::new_spanned(
                len_expr,
                "array length must be an integer literal",
            )),
        },
        _ => Err(syn::Error::new_spanned(
            len_expr,
            "array length must be an integer literal",
        )),
    }
}
