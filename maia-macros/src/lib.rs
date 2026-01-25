use proc_macro::TokenStream;

mod database;

#[proc_macro_derive(Crud, attributes(table_name, primary_key, skip_crud))]
pub fn derive_crud(input: TokenStream) -> TokenStream {
    database::create_derive_crud(input)
}
