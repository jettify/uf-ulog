use proc_macro::TokenStream;
use quote::{quote, ToTokens};
use syn::{parse_macro_input, Attribute, DeriveInput, Fields, LitStr, Type};

struct FieldInfo {
    name: String,
    ulog_type: &'static str,
    size: usize,
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
    let size_terms: Vec<usize> = field_infos.iter().map(|fi| fi.size).collect();

    let message_name = extract_ulog_name(input);

    Ok(generate_impl(
        ident,
        &message_name,
        &format_string,
        &size_terms,
    ))
}

fn extract_ulog_name(input: &DeriveInput) -> LitStr {
    input
        .attrs
        .iter()
        .find(|attr| attr.path().is_ident("uf_ulog"))
        .and_then(|attr| {
            let mut name = None;
            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("name") {
                    name = Some(meta.value()?.parse()?);
                }
                Ok(())
            })
            .ok()?;
            name
        })
        .unwrap_or_else(|| LitStr::new(&input.ident.to_string(), input.ident.span()))
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
    let (ulog_type, size) = primitive_spec(&field.ty)?;

    Ok(FieldInfo {
        name: field_name.to_string(),
        ulog_type,
        size,
    })
}

fn build_format_string(field_infos: &[FieldInfo]) -> String {
    field_infos
        .iter()
        .map(|fi| format!("{} {};", fi.ulog_type, fi.name))
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
    ty.to_token_stream().to_string().replace(' ', "") == "u64"
}

fn primitive_spec(ty: &Type) -> Result<(&'static str, usize), syn::Error> {
    let ty_str = ty.to_token_stream().to_string().replace(' ', "");

    let (ulog_type_name, sz) = match ty_str.as_str() {
        "u8" => ("uint8_t", 1usize),
        "i8" => ("int8_t", 1usize),
        "u16" => ("uint16_t", 2usize),
        "i16" => ("int16_t", 2usize),
        "u32" => ("uint32_t", 4usize),
        "i32" => ("int32_t", 4usize),
        "u64" => ("uint64_t", 8usize),
        "i64" => ("int64_t", 8usize),
        "f32" => ("float", 4usize),
        "f64" => ("double", 8usize),
        "bool" => ("bool", 1usize),
        "char" => ("char", 1usize),
        _ => {
            return Err(syn::Error::new_spanned(ty, "unsupported field type"));
        }
    };

    Ok((ulog_type_name, sz))
}
