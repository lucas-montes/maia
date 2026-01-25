use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, Data, DeriveInput, Fields, Lit, Meta};

/// Derive macro for automatic CRUD operations
///
/// # Attributes
/// - `#[table_name = "table_name"]` - Required: specifies the database table name
/// - `#[primary_key = "field_name"]` - Optional: defaults to "id"
/// - `#[skip_crud]` - On fields: Skip this field in INSERT/UPDATE operations
///
/// # Example
/// ```rust,ignore
/// use maia_macros::Crud;
/// use serde::{Serialize, Deserialize};
/// use chrono::{DateTime, Utc};
///
/// #[derive(Crud, Debug, Clone, Serialize, Deserialize)]
/// #[table_name = "products"]
/// pub struct Product {
///     #[skip_crud]
///     id: Option<i64>,
///     name: String,
///     calories: f64,
///     #[skip_crud]
///     created_at: DateTime<Utc>,
/// }
///
/// // Usage:
/// let conn = rusqlite::Connection::open_in_memory()?;
/// let product = Product { id: None, name: "Apple".to_string(), calories: 52.0, created_at: Utc::now() };
/// let id = Product::create(&conn, &product)?;
/// let found = Product::find(&conn, id)?;
/// ```

pub fn create_derive_crud(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    // Extract table name from attributes
    let table_name = extract_table_name(&input.attrs);
    let primary_key = extract_primary_key(&input.attrs).unwrap_or_else(|| "id".to_string());

    let struct_name = &input.ident;
    let generics = &input.generics;

    // Extract field information
    let fields = match &input.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(fields) => &fields.named,
            _ => panic!("Crud can only be derived for structs with named fields"),
        },
        _ => panic!("Crud can only be derived for structs"),
    };

    // Separate fields into crud fields and skip fields
    let mut crud_fields = Vec::new();
    let mut all_field_names = Vec::new();

    for field in fields {
        let field_name = field.ident.as_ref().unwrap();
        all_field_names.push(field_name);

        let skip = field.attrs.iter().any(|attr| {
            attr.path().is_ident("skip_crud")
        });

        if !skip {
            crud_fields.push(field_name);
        }
    }

    // Generate field names and placeholders for SQL
    let field_names_str = crud_fields
        .iter()
        .map(|f| f.to_string())
        .collect::<Vec<_>>()
        .join(", ");

    let placeholders = (1..=crud_fields.len())
        .map(|i| format!("?{}", i))
        .collect::<Vec<_>>()
        .join(", ");

    let update_set = crud_fields
        .iter()
        .enumerate()
        .map(|(i, f)| format!("{} = ?{}", f, i + 1))
        .collect::<Vec<_>>()
        .join(", ");

    let select_fields_str = all_field_names
        .iter()
        .map(|f| f.to_string())
        .collect::<Vec<_>>()
        .join(", ");

    // Generate INSERT SQL
    let insert_sql = format!(
        "INSERT INTO {} ({}) VALUES ({})",
        table_name, field_names_str, placeholders
    );

    // Generate UPDATE SQL
    let update_sql = format!(
        "UPDATE {} SET {} WHERE {} = ?{}",
        table_name,
        update_set,
        primary_key,
        crud_fields.len() + 1
    );

    // Generate SELECT SQL
    let select_sql = format!(
        "SELECT {} FROM {} WHERE {} = ?1",
        select_fields_str, table_name, primary_key
    );

    let select_all_sql = format!(
        "SELECT {} FROM {}",
        select_fields_str, table_name
    );

    // Generate DELETE SQL
    let delete_sql = format!(
        "DELETE FROM {} WHERE {} = ?1",
        table_name, primary_key
    );

    // Generate from_row code
    let from_row_assignments: Vec<_> = all_field_names.iter().enumerate().map(|(i, field)| {
        quote! {
            #field: row.get(#i)?
        }
    }).collect();

    let primary_key_ident = syn::Ident::new(&primary_key, proc_macro2::Span::call_site());

    // Generate parameter references for execute
    let insert_params: Vec<_> = crud_fields.iter().map(|field| {
        quote! { &entity.#field }
    }).collect();

    let update_params_with_id: Vec<_> = crud_fields.iter().map(|field| {
        quote! { &entity.#field }
    }).collect();

    // Generate the implementation
    let expanded = quote! {
        impl #generics #struct_name #generics {
            /// Insert a new record into the database
            pub fn create(conn: &rusqlite::Connection, entity: &Self) -> rusqlite::Result<i64> {
                conn.execute(
                    #insert_sql,
                    rusqlite::params![#(#insert_params),*]
                )?;
                Ok(conn.last_insert_rowid())
            }

            /// Find a record by primary key
            pub fn find(conn: &rusqlite::Connection, id: i64) -> rusqlite::Result<Option<Self>> {
                let mut stmt = conn.prepare_cached(#select_sql)?;
                let mut rows = stmt.query([id])?;

                if let Some(row) = rows.next()? {
                    Ok(Some(Self {
                        #(#from_row_assignments),*
                    }))
                } else {
                    Ok(None)
                }
            }

            /// Update an existing record
            pub fn update(entity: &Self, conn: &rusqlite::Connection) -> rusqlite::Result<()> {
                let id = entity.#primary_key_ident.ok_or_else(|| {
                    rusqlite::Error::InvalidQuery
                })?;

                conn.execute(
                    #update_sql,
                    rusqlite::params![#(#update_params_with_id),*, &id]
                )?;
                Ok(())
            }

            /// Delete a record by primary key
            pub fn delete(conn: &rusqlite::Connection, id: i64) -> rusqlite::Result<()> {
                conn.execute(#delete_sql, [id])?;
                Ok(())
            }

            /// List all records
            pub fn list(conn: &rusqlite::Connection) -> rusqlite::Result<Vec<Self>> {
                let mut stmt = conn.prepare_cached(#select_all_sql)?;
                let rows = stmt.query_map([], |row| {
                    Ok(Self {
                        #(#from_row_assignments),*
                    })
                })?;

                rows.collect()
            }

            /// Get the table name
            pub fn table_name() -> &'static str {
                #table_name
            }
        }
    };

    TokenStream::from(expanded)
}

fn extract_table_name(attrs: &[syn::Attribute]) -> String {
    for attr in attrs {
        if attr.path().is_ident("table_name") {
            if let Meta::NameValue(meta) = &attr.meta {
                if let syn::Expr::Lit(expr_lit) = &meta.value {
                    if let Lit::Str(lit_str) = &expr_lit.lit {
                        return lit_str.value();
                    }
                }
            }
        }
    }
    panic!("table_name attribute is required for Crud derive macro");
}

fn extract_primary_key(attrs: &[syn::Attribute]) -> Option<String> {
    for attr in attrs {
        if attr.path().is_ident("primary_key") {
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
