use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, DeriveInput, Expr, Fields, Lit, LitStr, Type, TypeArray, TypePath};

#[derive(Clone, Copy)]
enum PrimitiveKind {
    U8,
    I8,
    U16,
    I16,
    U32,
    I32,
    U64,
    I64,
    F32,
    F64,
    Bool,
}

impl PrimitiveKind {
    fn size(self) -> usize {
        match self {
            Self::U8 | Self::I8 | Self::Bool => 1,
            Self::U16 | Self::I16 => 2,
            Self::U32 | Self::I32 | Self::F32 => 4,
            Self::U64 | Self::I64 | Self::F64 => 8,
        }
    }

    fn ulog_name(self) -> &'static str {
        match self {
            Self::U8 => "uint8_t",
            Self::I8 => "int8_t",
            Self::U16 => "uint16_t",
            Self::I16 => "int16_t",
            Self::U32 => "uint32_t",
            Self::I32 => "int32_t",
            Self::U64 => "uint64_t",
            Self::I64 => "int64_t",
            Self::F32 => "float",
            Self::F64 => "double",
            Self::Bool => "bool",
        }
    }
}

enum ULogType {
    Scalar {
        kind: PrimitiveKind,
    },
    Array {
        elem_kind: PrimitiveKind,
        len: usize,
    },
}

impl ULogType {
    fn size(&self) -> usize {
        match self {
            Self::Scalar { kind } => kind.size(),
            Self::Array { elem_kind, len } => elem_kind.size() * len,
        }
    }

    fn format_ulog(&self) -> String {
        match self {
            Self::Scalar { kind } => kind.ulog_name().to_string(),
            Self::Array { elem_kind, len } => format!("{}[{len}]", elem_kind.ulog_name()),
        }
    }
}

struct FieldInfo {
    ident: syn::Ident,
    name: String,
    ty: ULogType,
}

#[proc_macro_derive(ULogData, attributes(uf_ulog))]
pub fn derive_ulog_message(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    expand_derive(&input)
        .unwrap_or_else(|err| err.to_compile_error())
        .into()
}

fn expand_derive(input: &DeriveInput) -> syn::Result<proc_macro2::TokenStream> {
    let ident = &input.ident;
    if !input.generics.params.is_empty() {
        return Err(syn::Error::new_spanned(
            &input.generics,
            "ULogData does not support structs with generics yet; remove generic parameters or implement `::uf_ulog::ULogData` manually",
        ));
    }
    let fields = extract_named_fields(&input.data, ident)?;
    validate_timestamp_field(fields, ident)?;

    let field_infos: Vec<FieldInfo> = fields.iter().map(process_field).collect::<Result<_, _>>()?;

    let format_string = build_format_string(&field_infos);
    let size_terms: Vec<usize> = field_infos.iter().map(|fi| fi.ty.size()).collect();
    let encode_body = build_encode_body(&field_infos);

    let message_name = extract_ulog_name(input)?;

    Ok(generate_impl(
        ident,
        &message_name,
        &format_string,
        &size_terms,
        &encode_body,
    ))
}

fn extract_ulog_name(input: &DeriveInput) -> syn::Result<LitStr> {
    let mut name = None;
    for attr in input
        .attrs
        .iter()
        .filter(|attr| attr.path().is_ident("uf_ulog"))
    {
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
                "ULogData requires a struct with named fields",
            )),
        },
        _ => Err(syn::Error::new_spanned(
            ident,
            "Only structs can derive ULogData",
        )),
    }
}

fn validate_timestamp_field(
    fields: &syn::punctuated::Punctuated<syn::Field, syn::token::Comma>,
    ident: &syn::Ident,
) -> syn::Result<()> {
    let timestamp_field = fields
        .iter()
        .find(|f| f.ident.as_ref().is_some_and(|id| id == "timestamp"))
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
        ident: field_name.clone(),
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
    encode_body: &[proc_macro2::TokenStream],
) -> proc_macro2::TokenStream {
    let format_lit = LitStr::new(format_string, ident.span());

    quote! {
        impl ::uf_ulog::ULogData for #ident {
            const NAME: &'static str = #message_name;
            const WIRE_SIZE: usize = 0 #(+ #size_terms)*;
            const FORMAT: &'static str = #format_lit;

            #[inline]
            fn encode(&self, buf: &mut [u8]) -> ::core::result::Result<(), ::uf_ulog::EncodeError> {
                if buf.len() < Self::WIRE_SIZE {
                    return Err(::uf_ulog::EncodeError::BufferOverflow);
                }
                let mut offset = 0usize;
                #(#encode_body)*
                Ok(())
            }
            #[inline]
            fn timestamp(&self) -> u64 {
                self.timestamp
            }
        }
    }
}

fn build_encode_body(field_infos: &[FieldInfo]) -> Vec<proc_macro2::TokenStream> {
    field_infos
        .iter()
        .map(|field| {
            let field_ident = &field.ident;
            match field.ty {
                ULogType::Scalar { kind } => {
                    encode_primitive_tokens(quote!(self.#field_ident), kind)
                }
                ULogType::Array { elem_kind, .. } => {
                    let elem_tokens = encode_primitive_tokens(quote!(*value), elem_kind);
                    quote! {
                        for value in &self.#field_ident {
                            #elem_tokens
                        }
                    }
                }
            }
        })
        .collect()
}

fn encode_primitive_tokens(
    value_tokens: proc_macro2::TokenStream,
    kind: PrimitiveKind,
) -> proc_macro2::TokenStream {
    match kind {
        PrimitiveKind::U8 => quote! {
            buf[offset] = #value_tokens;
            offset += 1;
        },
        PrimitiveKind::I8 => quote! {
            buf[offset] = (#value_tokens) as u8;
            offset += 1;
        },
        PrimitiveKind::Bool => quote! {
            buf[offset] = u8::from(#value_tokens);
            offset += 1;
        },
        PrimitiveKind::U16
        | PrimitiveKind::I16
        | PrimitiveKind::U32
        | PrimitiveKind::I32
        | PrimitiveKind::U64
        | PrimitiveKind::I64
        | PrimitiveKind::F32
        | PrimitiveKind::F64 => quote! {
            let bytes = (#value_tokens).to_le_bytes();
            let end = offset + bytes.len();
            buf[offset..end].copy_from_slice(&bytes);
            offset = end;
        },
    }
}

fn is_u64_type(ty: &Type) -> bool {
    primitive_type_name(ty).is_some_and(|ident| ident == "u64")
}

fn primitive_type_name(ty: &Type) -> Option<&syn::Ident> {
    match ty {
        Type::Path(type_path) if is_simple_path(type_path) => primitive_ident_from_path(type_path),
        _ => None,
    }
}

fn is_simple_path(type_path: &TypePath) -> bool {
    type_path.qself.is_none() && type_path.path.segments.len() == 1
}

fn primitive_ident_from_path(type_path: &TypePath) -> Option<&syn::Ident> {
    type_path
        .path
        .segments
        .first()
        .map(|segment| &segment.ident)
}

fn primitive_spec(ident: &syn::Ident) -> Option<PrimitiveKind> {
    match ident.to_string().as_str() {
        "u8" => Some(PrimitiveKind::U8),
        "i8" => Some(PrimitiveKind::I8),
        "u16" => Some(PrimitiveKind::U16),
        "i16" => Some(PrimitiveKind::I16),
        "u32" => Some(PrimitiveKind::U32),
        "i32" => Some(PrimitiveKind::I32),
        "u64" => Some(PrimitiveKind::U64),
        "i64" => Some(PrimitiveKind::I64),
        "f32" => Some(PrimitiveKind::F32),
        "f64" => Some(PrimitiveKind::F64),
        "bool" => Some(PrimitiveKind::Bool),
        _ => None,
    }
}

fn type_spec(ty: &Type) -> Result<ULogType, syn::Error> {
    match ty {
        Type::Array(array_ty) => parse_array_type(array_ty),
        Type::Path(type_path) if is_simple_path(type_path) => parse_primitive_type(type_path),
        _ => Err(unsupported_type_error(ty)),
    }
}

fn parse_primitive_type(type_path: &TypePath) -> Result<ULogType, syn::Error> {
    let Some(ident) = primitive_ident_from_path(type_path) else {
        return Err(unsupported_type_error(&Type::Path(type_path.clone())));
    };
    let Some(kind) = primitive_spec(ident) else {
        return Err(unsupported_type_error(&Type::Path(type_path.clone())));
    };
    Ok(ULogType::Scalar { kind })
}

fn parse_array_type(array_ty: &TypeArray) -> Result<ULogType, syn::Error> {
    let elem_ident = primitive_type_name(&array_ty.elem)
        .ok_or_else(|| syn::Error::new_spanned(&array_ty.elem, "unsupported array element type"))?;
    let elem_kind = primitive_spec(elem_ident)
        .ok_or_else(|| syn::Error::new_spanned(&array_ty.elem, "unsupported array element type"))?;
    let len = extract_array_len(&array_ty.len)?;

    Ok(ULogType::Array { elem_kind, len })
}

fn unsupported_type_error(ty: &Type) -> syn::Error {
    syn::Error::new_spanned(ty, "unsupported field type")
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
