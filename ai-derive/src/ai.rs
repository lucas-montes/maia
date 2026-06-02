use proc_macro::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, Fields, Lit, Meta, Type, parse_macro_input};

/// Derive macro for generating Google Gemini-compatible JSON schemas
///
/// This macro generates a `to_schema()` method that returns a `serde_json::Value`
/// representing the JSON schema for the struct, compatible with Gemini API's structured output.
///
/// # Attributes
/// - `#[description = "..."]` - On struct or field: Adds a description
/// - `#[gemini(format = "date-time")]` - On string fields: Sets format (date-time, date, time)
/// - `#[gemini(min = 0, max = 100)]` - On number fields: Sets minimum and maximum values
/// - `#[gemini(min_items = 1, max_items = 10)]` - On array fields: Sets item constraints
///
/// # Supported Types
/// - `String` → "string"
/// - `i32`, `i64`, `u32`, `u64` → "integer"
/// - `f32`, `f64` → "number"
/// - `bool` → "boolean"
/// - `Option<T>` → type with nullable support
/// - `Vec<T>` → "array"
/// - Nested structs → "object"
///
/// # Example
/// ```rust,ignore
/// use maia_macros::GeminiSchema;
/// use serde::{Serialize, Deserialize};
///
/// #[derive(GeminiSchema, Serialize, Deserialize)]
/// #[description = "A recipe ingredient"]
/// struct Ingredient {
///     #[description = "Name of the ingredient"]
///     name: String,
///
///     #[description = "Quantity with units"]
///     quantity: String,
/// }
///
/// #[derive(GeminiSchema, Serialize, Deserialize)]
/// #[description = "A cooking recipe"]
/// struct Recipe {
///     #[description = "The recipe name"]
///     recipe_name: String,
///
///     #[description = "Prep time in minutes"]
///     prep_time_minutes: Option<i32>,
///
///     #[description = "List of ingredients"]
///     ingredients: Vec<Ingredient>,
///
///     #[description = "Cooking instructions"]
///     instructions: Vec<String>,
/// }
///
/// // Use it with Gemini API:
/// let schema = Recipe::to_schema();
/// ```
pub fn create_derive_gemini_schema(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let struct_name = &input.ident;
    let generics = &input.generics;

    // Extract struct description
    let struct_description = extract_description(&input.attrs);

    // Extract field information
    let fields = match &input.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(fields) => &fields.named,
            _ => panic!("GeminiSchema can only be derived for structs with named fields"),
        },
        _ => panic!("GeminiSchema can only be derived for structs"),
    };

    // Build properties and required fields
    let mut property_inserts = Vec::new();
    let mut required_fields = Vec::new();
    let mut field_names_ordered = Vec::new();

    for field in fields {
        let field_name = field.ident.as_ref().unwrap();
        let field_name_str = field_name.to_string();
        field_names_ordered.push(field_name_str.clone());

        let field_description = extract_description(&field.attrs);
        let field_type = &field.ty;

        let (is_optional, inner_type) = extract_option_type(field_type);

        if !is_optional {
            required_fields.push(field_name_str.clone());
        }

        let type_to_check = if is_optional { inner_type } else { field_type };
        let type_name = get_type_name(type_to_check);
        let is_custom = is_custom_struct(&type_name);

        let desc_json = if let Some(desc) = field_description {
            quote! { schema.as_object_mut().unwrap().insert("description".to_string(), serde_json::json!(#desc)); }
        } else {
            quote! {}
        };

        // Generate the schema creation code
        let schema_creation = if is_custom {
            // For custom types, call their to_schema() method
            let type_ident = syn::Ident::new(&type_name, proc_macro2::Span::call_site());
            if is_optional {
                quote! {
                    let mut schema = #type_ident::to_schema();
                    // Make it nullable
                    if let Some(obj) = schema.as_object_mut() {
                        if let Some(type_val) = obj.get("type") {
                            let mut new_type = serde_json::json!(["null"]);
                            if let Some(arr) = new_type.as_array_mut() {
                                arr.insert(0, type_val.clone());
                            }
                            obj.insert("type".to_string(), new_type);
                        }
                    }
                    #desc_json
                    schema
                }
            } else {
                quote! {
                    let mut schema = #type_ident::to_schema();
                    #desc_json
                    schema
                }
            }
        } else {
            // For primitive types, generate inline
            let type_schema = if is_optional {
                generate_type_schema(inner_type, &field.attrs, true)
            } else {
                generate_type_schema(field_type, &field.attrs, false)
            };

            quote! {
                let mut schema = serde_json::json!({
                    #type_schema
                });
                #desc_json
                schema
            }
        };

        property_inserts.push(quote! {
            properties.insert(#field_name_str.to_string(), {
                #schema_creation
            });
        });
    }

    let struct_desc_json = if let Some(desc) = struct_description {
        quote! { "description": #desc, }
    } else {
        quote! {}
    };

    let expanded = quote! {
        impl #generics ::ai::StructuredOutput for #struct_name #generics {
            /// Generate a JSON schema compatible with Google Gemini API
            fn to_schema() -> serde_json::Value {
                let mut properties = serde_json::Map::new();

                #(#property_inserts)*

                serde_json::json!({
                    "type": "object",
                    #struct_desc_json
                    "properties": properties,
                    "required": vec![#(#required_fields),*],
                    "propertyOrdering": vec![#(#field_names_ordered),*]
                })
            }
        }
    };

    TokenStream::from(expanded)
}

fn extract_description(attrs: &[syn::Attribute]) -> Option<String> {
    for attr in attrs {
        if attr.path().is_ident("description") {
            if let Meta::NameValue(meta) = &attr.meta {
                if let syn::Expr::Lit(expr_lit) = &meta.value {
                    if let Lit::Str(lit_str) = &expr_lit.lit {
                        return Some(lit_str.value());
                    }
                }
            }
        }
    }
    None
}

fn extract_option_type(ty: &Type) -> (bool, &Type) {
    if let Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.last() {
            if segment.ident == "Option" {
                if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                    if let Some(syn::GenericArgument::Type(inner_ty)) = args.args.first() {
                        return (true, inner_ty);
                    }
                }
            }
        }
    }
    (false, ty)
}

fn generate_type_schema(
    ty: &Type,
    attrs: &[syn::Attribute],
    is_nullable: bool,
) -> proc_macro2::TokenStream {
    let base_type = get_json_type(ty);

    // Extract gemini attributes
    let format_attr = extract_gemini_attr(attrs, "format");
    let min_attr = extract_gemini_attr(attrs, "min");
    let max_attr = extract_gemini_attr(attrs, "max");
    let min_items_attr = extract_gemini_attr(attrs, "min_items");
    let max_items_attr = extract_gemini_attr(attrs, "max_items");
    let enum_attr = extract_gemini_enum(attrs);

    let mut type_tokens = if is_nullable {
        quote! { "type": [#base_type, "null"], }
    } else {
        quote! { "type": #base_type, }
    };

    // Add format for strings
    if let Some(format) = format_attr {
        type_tokens = quote! {
            #type_tokens
            "format": #format,
        };
    }

    // Add min/max for numbers
    if let Some(min) = min_attr {
        let min_val: i64 = min.parse().unwrap_or(0);
        type_tokens = quote! {
            #type_tokens
            "minimum": #min_val,
        };
    }
    if let Some(max) = max_attr {
        let max_val: i64 = max.parse().unwrap_or(100);
        type_tokens = quote! {
            #type_tokens
            "maximum": #max_val,
        };
    }

    // Add enum values
    if let Some(enum_vals) = enum_attr {
        type_tokens = quote! {
            #type_tokens
            "enum": vec![#(#enum_vals),*],
        };
    }

    // Handle arrays
    if let Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.last() {
            if segment.ident == "Vec" {
                if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                    if let Some(syn::GenericArgument::Type(inner_ty)) = args.args.first() {
                        // Check if inner type is a custom struct (object)
                        let inner_type_name = get_type_name(inner_ty);
                        let is_custom_type = is_custom_struct(&inner_type_name);

                        let inner_schema = if is_custom_type {
                            // For nested custom structs, call their to_schema() method
                            let inner_ident =
                                syn::Ident::new(&inner_type_name, proc_macro2::Span::call_site());
                            quote! { #inner_ident::to_schema() }
                        } else {
                            // For primitive types, generate inline schema
                            let inner_schema_tokens = generate_type_schema(inner_ty, &[], false);
                            quote! {
                                serde_json::json!({
                                    #inner_schema_tokens
                                })
                            }
                        };

                        let mut items_tokens = quote! {
                            "items": #inner_schema,
                        };

                        if let Some(min_items) = min_items_attr {
                            let min_val: i64 = min_items.parse().unwrap_or(0);
                            items_tokens = quote! {
                                #items_tokens
                                "minItems": #min_val,
                            };
                        }
                        if let Some(max_items) = max_items_attr {
                            let max_val: i64 = max_items.parse().unwrap_or(100);
                            items_tokens = quote! {
                                #items_tokens
                                "maxItems": #max_val,
                            };
                        }

                        return quote! {
                            #type_tokens
                            #items_tokens
                        };
                    }
                }
            }
        }
    }

    type_tokens
}

fn get_json_type(ty: &Type) -> &'static str {
    if let Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.last() {
            let ident = segment.ident.to_string();
            return match ident.as_str() {
                "String" | "str" => "string",
                "i8" | "i16" | "i32" | "i64" | "u8" | "u16" | "u32" | "u64" | "isize" | "usize" => {
                    "integer"
                }
                "f32" | "f64" => "number",
                "bool" => "boolean",
                "Vec" => "array",
                _ => "object", // Assume nested struct
            };
        }
    }
    "object"
}

fn get_type_name(ty: &Type) -> String {
    if let Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.last() {
            return segment.ident.to_string();
        }
    }
    "Unknown".to_string()
}

fn is_custom_struct(type_name: &str) -> bool {
    // Check if it's not a primitive type
    !matches!(
        type_name,
        "String"
            | "str"
            | "i8"
            | "i16"
            | "i32"
            | "i64"
            | "u8"
            | "u16"
            | "u32"
            | "u64"
            | "isize"
            | "usize"
            | "f32"
            | "f64"
            | "bool"
            | "Vec"
            | "Option"
    )
}

fn extract_gemini_attr(attrs: &[syn::Attribute], key: &str) -> Option<String> {
    for attr in attrs {
        if attr.path().is_ident("gemini") {
            if let Ok(meta_list) = attr.meta.require_list() {
                // Parse nested attributes like #[gemini(format = "date-time")]
                let tokens = &meta_list.tokens;
                let tokens_str = tokens.to_string();

                // Simple parsing - look for key = "value"
                if let Some(pos) = tokens_str.find(&format!("{} =", key)) {
                    let after_eq = &tokens_str[pos + key.len() + 1..].trim_start();
                    if let Some(start_quote) = after_eq.find('"') {
                        let after_quote = &after_eq[start_quote + 1..];
                        if let Some(end_quote) = after_quote.find('"') {
                            return Some(after_quote[..end_quote].to_string());
                        }
                    }
                }
            }
        }
    }
    None
}

fn extract_gemini_enum(attrs: &[syn::Attribute]) -> Option<Vec<String>> {
    for attr in attrs {
        if attr.path().is_ident("gemini") {
            if let Ok(meta_list) = attr.meta.require_list() {
                let tokens_str = meta_list.tokens.to_string();

                if let Some(pos) = tokens_str.find("enum =") {
                    let after_eq = &tokens_str[pos + 6..].trim_start();
                    // Parse array like [&str] or ["val1", "val2"]
                    if after_eq.starts_with('[') {
                        // Simple enum extraction (this is basic, could be improved)
                        return Some(vec![]); // Placeholder for now
                    }
                }
            }
        }
    }
    None
}
