use utoipa::OpenApi;

#[derive(OpenApi)]
#[openapi(
    info(title = "Maia Sync Server", version = "1.0.0", description = "FitFat offline-first sync — single maia.db v30, UUID v7, Bearer auth, since cursors, deleted[] tombstones"),
    paths(),
    components(schemas()),
    security(
        ("bearerAuth" = [])
    ),
    modifiers(&SecurityAddon)
)]
pub struct ApiDoc;

struct SecurityAddon;
impl utoipa::Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        if let Some(components) = openapi.components.as_mut() {
            components.add_security_scheme("bearerAuth", utoipa::openapi::security::SecurityScheme::Http(utoipa::openapi::security::Http::new(utoipa::openapi::security::HttpAuthScheme::Bearer)));
        }
    }
}
