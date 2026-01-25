use proc_macro::TokenStream;

mod ai;


#[proc_macro_derive(StructuredOutput, attributes(description, gemini))]
pub fn derive_gemini_schema(input: TokenStream) -> TokenStream {
    ai::create_derive_gemini_schema(input)
}
