//! Desktop-only account maintenance: the provider identity.
//!
//! `accounts.source` / `accounts.external_id` say which provider account a row
//! *is*, and they are deliberately off the phone sync surface — `pull_accounts`
//! and `push_budget_accounts` both name explicit column lists so a phone can
//! neither read nor clear them. That left no way to set them at all: an account
//! created by hand could never be told which provider account it was, so
//! statements could never snap onto it.
//!
//! This endpoint is that way, and it is deliberately *not* `/budget-accounts`:
//! extending that handler's `ON CONFLICT DO UPDATE` column list would put
//! identity on the sync surface and let a phone push clear it, which is
//! precisely what keeping the columns server-only set out to prevent.
//!
//! Re-pointing an identity is safe and needs no guard. Since
//! [`crate::statements::store::resolve_account`] stopped deriving the account
//! id from `(source, external_id)`, moving a number keeps the account row, its
//! name and every transaction already imported onto it; only where the *next*
//! import looks changes.

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use rusqlite::OptionalExtension;
use serde::Deserialize;

use crate::{db, error::ErrorBody, server::AppState, statements::SourceId};

type ApiError = (StatusCode, Json<ErrorBody>);

fn error(status: StatusCode, message: impl Into<String>) -> ApiError {
    (
        status,
        Json(ErrorBody {
            message: message.into(),
        }),
    )
}

/// Either both fields or neither: an identity is the pair, and half of one
/// would never match anything.
#[derive(Debug, Deserialize)]
pub struct IdentityRequest {
    #[serde(default)]
    pub source: Option<String>,
    #[serde(rename = "externalId", default)]
    pub external_id: Option<String>,
}

/// `PUT /accounts/:id/identity`
pub async fn put_account_identity(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<IdentityRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let source = request.source.as_deref().map(str::trim).filter(|s| !s.is_empty());
    let external_id = request
        .external_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());

    let identity = match (source, external_id) {
        (None, None) => None,
        (Some(source), Some(external_id)) => {
            let parsed = SourceId::parse(source).ok_or_else(|| {
                error(
                    StatusCode::BAD_REQUEST,
                    format!("unknown statement source {source:?}"),
                )
            })?;
            Some((parsed.as_str().to_string(), external_id.to_string()))
        }
        _ => {
            return Err(error(
                StatusCode::BAD_REQUEST,
                "a provider identity needs both a source and a number; send both as null to clear it",
            ));
        }
    };

    let mut conn = state
        .db
        .get()
        .map_err(|e| error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let tx = conn
        .transaction()
        .map_err(|e| error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // `.optional()?` rather than `.ok()`: a busy database must not be reported
    // as "account not found", and below it must not be reported as "no
    // conflict" — which would let the UPDATE fail with a raw SQLITE_CONSTRAINT
    // as a 500, the very thing the pre-check exists to avoid.
    let existing: Option<Option<i64>> = tx
        .query_row(
            "SELECT deleted_at FROM accounts WHERE id=?1",
            rusqlite::params![id],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    match existing {
        None => return Err(error(StatusCode::NOT_FOUND, format!("account {id} not found"))),
        // Clearing is always allowed, including on a tombstone: a deleted
        // account still holds its number against the partial unique index, so
        // refusing here would leave that number unusable by anything, with no
        // way back — the tombstone is invisible on the accounts screen.
        Some(Some(_)) if identity.is_some() => {
            return Err(error(
                StatusCode::BAD_REQUEST,
                format!("account {id} is deleted; restore it before setting its number"),
            ));
        }
        Some(_) => {}
    }

    // The partial unique index would reject this anyway; checking first is what
    // turns a raw SQLITE_CONSTRAINT into an answer the user can act on.
    if let Some((source, external_id)) = identity.as_ref() {
        let holder: Option<(String, String, bool)> = tx
            .query_row(
                "SELECT id, name, deleted_at IS NOT NULL FROM accounts
                 WHERE source=?1 AND external_id=?2 AND id<>?3",
                rusqlite::params![source, external_id, id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()
            .map_err(|e| error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        if let Some((other_id, other_name, deleted)) = holder {
            // Say when the holder is a tombstone. Otherwise the refusal names
            // an account the user cannot find anywhere to clear it from.
            let where_ = if deleted {
                " (a deleted account)"
            } else {
                ""
            };
            return Err(error(
                StatusCode::CONFLICT,
                format!(
                    "{source} {external_id} already belongs to {other_name} ({other_id}){where_}; \
                     clear it there first"
                ),
            ));
        }
    }

    // A leaf's source must equal its root's: an account filed under Crédit
    // Agricole cannot carry Firstrade's number, or the tree would say one thing
    // and the identity another.
    let (root_id, root_source, root_name): (Option<String>, Option<String>, Option<String>) = tx
        .query_row(
            "SELECT r.id, r.source, r.name FROM accounts a
             LEFT JOIN account_roots r ON r.id = a.root_id
             WHERE a.id=?1",
            rusqlite::params![id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()
        .map_err(|e| error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .unwrap_or((None, None, None));

    let now = db::server_time_ms();
    let root_after: Option<String> = match identity.as_ref() {
        Some((source, _)) => match (root_id.as_deref(), root_source.as_deref()) {
            // No root yet: the account joins the root for this source, which
            // is made if this is the first account of that institution. The
            // tree stays complete this way.
            (None, _) => Some(
                crate::statements::store::resolve_root(&tx, source, now)
                    .map_err(|e| error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?,
            ),
            // The root already names a source, and it has to be the same one.
            (Some(_), Some(root_source)) if root_source != *source => {
                let under = root_name.as_deref().unwrap_or("another institution");
                return Err(error(
                    StatusCode::CONFLICT,
                    format!("this account is under {under}, so it cannot carry a {source} number"),
                ));
            }
            // A root with no source of its own cannot hold a leaf that has
            // one: there would be no institution to file it under.
            (Some(_), None) => {
                return Err(error(
                    StatusCode::CONFLICT,
                    "this account is under a root with no source of its own, so it cannot carry a number",
                ));
            }
            (Some(id), Some(_)) => Some(id.to_string()),
        },
        // Clearing the identity leaves the root alone: the account is still
        // that institution's, it just has no provider number any more.
        None => root_id,
    };

    let (source, external_id) = match identity.as_ref() {
        Some((source, external_id)) => (Some(source.as_str()), Some(external_id.as_str())),
        None => (None, None),
    };
    tx.execute(
        "UPDATE accounts SET source=?1, external_id=?2, root_id=?3, updated_at=?4 WHERE id=?5",
        rusqlite::params![source, external_id, root_after, now, id],
    )
    .map_err(|e| error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Snap staged statements onto the account, the same resolution
    // `save_statement` runs at stage time. Without this the user would have to
    // re-drop the file to see "uses existing account".
    //
    // Clearing an identity deliberately leaves `account_id` alone: those
    // statements already resolved, and un-pointing them would strand work the
    // user can no longer redo from this screen.
    let resolved = match (source, external_id) {
        (Some(source), Some(external_id)) => tx
            .execute(
                "UPDATE bank_statements SET account_id=?1, updated_at=?2
                 WHERE source=?3 AND account_number=?4 AND deleted_at IS NULL
                   AND (account_id IS NULL OR account_id<>?1)",
                rusqlite::params![id, now, source, external_id],
            )
            .map_err(|e| error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?,
        _ => 0,
    };

    tx.commit()
        .map_err(|e| error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(serde_json::json!({
        "id": id,
        "source": source,
        "externalId": external_id,
        "statementsResolved": resolved,
    })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    fn state() -> AppState {
        AppState::new(
            Config::from_values(serde_json::Map::new(), serde_json::Map::new()),
            crate::db::init_memory().unwrap(),
        )
    }

    /// A hand-created account, as the accounts screen makes one: no identity.
    fn account(state: &AppState, id: &str, name: &str) {
        let conn = state.db.get().unwrap();
        conn.execute(
            "INSERT INTO accounts (id, name, type, opening_balance, note, created_at, updated_at)
             VALUES (?1, ?2, 'bank', 0.0, NULL, 1, 1)",
            rusqlite::params![id, name],
        )
        .unwrap();
    }

    fn identity_of(state: &AppState, id: &str) -> (Option<String>, Option<String>) {
        let conn = state.db.get().unwrap();
        conn.query_row(
            "SELECT source, external_id FROM accounts WHERE id=?1",
            rusqlite::params![id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap()
    }

    async fn put(
        state: &AppState,
        id: &str,
        source: Option<&str>,
        external_id: Option<&str>,
    ) -> Result<serde_json::Value, ApiError> {
        put_account_identity(
            State(state.clone()),
            Path(id.to_string()),
            Json(IdentityRequest {
                source: source.map(str::to_string),
                external_id: external_id.map(str::to_string),
            }),
        )
        .await
        .map(|Json(body)| body)
    }

    #[tokio::test]
    async fn a_number_can_be_set_on_a_hand_created_account_and_cleared_again() {
        let state = state();
        account(&state, "acc-1", "Firstrade");

        let body = put(&state, "acc-1", Some("firstrade"), Some("firstrade")).await.unwrap();
        assert_eq!(body["source"], "firstrade");
        assert_eq!(
            identity_of(&state, "acc-1"),
            (Some("firstrade".into()), Some("firstrade".into()))
        );

        put(&state, "acc-1", None, None).await.unwrap();
        assert_eq!(identity_of(&state, "acc-1"), (None, None));
    }

    /// An alias is accepted and stored canonically, so `ca` and
    /// `credit_agricole` cannot become two different identities.
    #[tokio::test]
    async fn a_source_alias_is_stored_canonically() {
        let state = state();
        account(&state, "acc-1", "Eko");
        put(&state, "acc-1", Some("ca"), Some("84890097587")).await.unwrap();
        assert_eq!(identity_of(&state, "acc-1").0.as_deref(), Some("credit_agricole"));
    }

    #[tokio::test]
    async fn a_conflicting_number_is_refused_with_the_account_that_holds_it() {
        let state = state();
        account(&state, "acc-1", "Eko by CA");
        account(&state, "acc-2", "Livret A");
        put(&state, "acc-1", Some("credit_agricole"), Some("123")).await.unwrap();

        let (status, Json(body)) =
            put(&state, "acc-2", Some("credit_agricole"), Some("123")).await.unwrap_err();
        assert_eq!(status, StatusCode::CONFLICT);
        assert!(
            body.message.contains("Eko by CA") && body.message.contains("acc-1"),
            "the refusal must name the account that holds the number: {}",
            body.message
        );
        assert_eq!(identity_of(&state, "acc-2"), (None, None), "nothing changed");
    }

    #[tokio::test]
    async fn half_an_identity_is_refused() {
        let state = state();
        account(&state, "acc-1", "Firstrade");
        let (status, _) = put(&state, "acc-1", Some("firstrade"), None).await.unwrap_err();
        assert_eq!(status, StatusCode::BAD_REQUEST);
        let (status, _) = put(&state, "acc-1", None, Some("firstrade")).await.unwrap_err();
        assert_eq!(status, StatusCode::BAD_REQUEST);
        let (status, _) = put(&state, "acc-1", Some("natwest"), Some("1")).await.unwrap_err();
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    /// Moving a number is a clear-then-set, and it strands nothing: the
    /// account keeps its id, its name and every transaction already imported
    /// onto it. Only where the *next* import looks changes. This is the whole
    /// point of T03 having removed the derived account id.
    #[tokio::test]
    async fn moving_a_number_to_another_account_strands_no_transactions() {
        let state = state();
        account(&state, "imported", "Firstrade");
        account(&state, "mine", "My broker");
        put(&state, "imported", Some("firstrade"), Some("firstrade"))
            .await
            .unwrap();

        {
            let conn = state.db.get().unwrap();
            for (id, amount) in [("tx-1", 10.0), ("tx-2", 20.0)] {
                conn.execute(
                    "INSERT INTO transactions
                        (id, type, amount, currency_code, amount_base, rate_used, account_id,
                         category, date, note, is_draft, created_at, updated_at)
                     VALUES (?1,'income',?2,'USD',?2,1.0,'imported',NULL,1,'',0,1,1)",
                    rusqlite::params![id, amount],
                )
                .unwrap();
            }
            conn.execute(
                "INSERT INTO bank_statements
                    (id, source, institution, account_name, account_number, account_id,
                     currency_code, source_hash, source_key, status, warnings,
                     created_at, updated_at)
                 VALUES ('st-1','firstrade','Firstrade','Firstrade','firstrade','imported',
                         'USD','h','firstrade:firstrade:h','staged','[]',1,1)",
                [],
            )
            .unwrap();
        }

        // Taking the number while another account still holds it is refused,
        // naming the holder — that is the guard, not a reason moving is unsafe.
        let (status, _) = put(&state, "mine", Some("firstrade"), Some("firstrade"))
            .await
            .unwrap_err();
        assert_eq!(status, StatusCode::CONFLICT);

        put(&state, "imported", None, None).await.unwrap();
        let body = put(&state, "mine", Some("firstrade"), Some("firstrade"))
            .await
            .unwrap();
        assert_eq!(
            body["statementsResolved"], 1,
            "staged statements of that identity snap onto the account"
        );

        let conn = state.db.get().unwrap();
        let (name, still_live): (String, bool) = conn
            .query_row(
                "SELECT name, deleted_at IS NULL FROM accounts WHERE id='imported'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(name, "Firstrade", "the old account is untouched");
        assert!(still_live);
        let kept: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM transactions WHERE account_id='imported'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(kept, 2, "every transaction stays where it was booked");
        let statement_account: String = conn
            .query_row(
                "SELECT account_id FROM bank_statements WHERE id='st-1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(statement_account, "mine", "the next import looks here now");
    }

    /// The identity columns stay off the phone sync surface: a pull never
    /// mentions them, and a push round trip cannot clear them.
    #[tokio::test]
    async fn a_phone_round_trip_neither_reads_nor_clears_the_identity() {
        use axum::extract::Query;
        use axum::response::IntoResponse;

        let state = state();
        account(&state, "acc-1", "Eko by CA");
        put(&state, "acc-1", Some("credit_agricole"), Some("84890097587"))
            .await
            .unwrap();

        let pulled = crate::handlers::pull::pull_accounts(
            State(state.clone()),
            Query(crate::handlers::pull::SinceQuery { since: Some(0) }),
        )
        .await
        .into_response();
        let bytes = axum::body::to_bytes(pulled.into_body(), usize::MAX)
            .await
            .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let account = &body["items"][0];
        assert_eq!(account["id"], "acc-1");
        assert!(
            account.get("source").is_none() && account.get("externalId").is_none(),
            "a phone must not even see the identity: {account}"
        );

        // The phone sends back what it has — which is the account without any
        // identity on it.
        let push: crate::handlers::push::BudgetAccountsPush = serde_json::from_value(
            serde_json::json!({
                "accounts": [{
                    "id": "acc-1",
                    "name": "Eko by CA (renamed on the phone)",
                    "type": "bank",
                    "openingBalance": 0.0,
                    "note": null,
                    "createdAt": 1,
                    "deletedAt": null,
                }],
                "deleted": [],
            }),
        )
        .unwrap();
        crate::handlers::push::push_budget_accounts(State(state.clone()), Json(push)).await;

        assert_eq!(
            identity_of(&state, "acc-1"),
            (
                Some("credit_agricole".into()),
                Some("84890097587".into())
            ),
            "the push updated the name but cannot touch the identity"
        );
        let name: String = state
            .db
            .get()
            .unwrap()
            .query_row("SELECT name FROM accounts WHERE id='acc-1'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(name, "Eko by CA (renamed on the phone)");
    }

    /// A root for a source, as the tree holds one.
    fn root(state: &AppState, id: &str, name: &str, source: &str) {
        let conn = state.db.get().unwrap();
        conn.execute(
            "INSERT INTO account_roots (id, name, kind, source, created_at, updated_at)
             VALUES (?1, ?2, 'bank', ?3, 1, 1)",
            rusqlite::params![id, name, source],
        )
        .unwrap();
    }

    /// An account under a root, as the tree holds one: a leaf carrying the
    /// institution's source and the root that source names.
    fn leaf(state: &AppState, id: &str, name: &str, source: &str, root_id: &str) {
        let conn = state.db.get().unwrap();
        conn.execute(
            "INSERT INTO accounts
                (id, name, type, opening_balance, note, created_at, updated_at,
                 source, external_id, root_id)
             VALUES (?1, ?2, 'cash', 0.0, NULL, 1, 1, ?3, ?4, ?5)",
            rusqlite::params![id, name, source, format!("{source}-{id}"), root_id],
        )
        .unwrap();
    }

    fn root_of(state: &AppState, id: &str) -> Option<String> {
        let conn = state.db.get().unwrap();
        conn.query_row(
            "SELECT root_id FROM accounts WHERE id=?1",
            rusqlite::params![id],
            |r| r.get(0),
        )
        .unwrap()
    }

    /// A leaf's source must equal its root's: an account filed under Crédit
    /// Agricole cannot carry Firstrade's number, or the tree would say one
    /// thing and the identity another.
    #[tokio::test]
    async fn a_number_from_another_institution_is_refused() {
        let state = state();
        root(&state, "r-ca", "Crédit Agricole", "credit_agricole");
        leaf(&state, "acc-1", "Eko by CA", "credit_agricole", "r-ca");

        let error = put(&state, "acc-1", Some("firstrade"), Some("abc123"))
            .await
            .unwrap_err();
        assert_eq!(error.0, axum::http::StatusCode::CONFLICT);
        assert!(
            error.1.message.contains("Crédit Agricole"),
            "the refusal names the root: {}",
            error.1.message
        );
        assert_eq!(
            identity_of(&state, "acc-1").0.as_deref(),
            Some("credit_agricole"),
            "the refused number did not land"
        );
    }

    /// The same institution's number is what the root is there for.
    #[tokio::test]
    async fn a_number_from_the_same_institution_is_accepted() {
        let state = state();
        root(&state, "r-ca", "Crédit Agricole", "credit_agricole");
        leaf(&state, "acc-1", "Eko by CA", "credit_agricole", "r-ca");

        put(
            &state,
            "acc-1",
            Some("credit_agricole"),
            Some("84890097587"),
        )
        .await
        .unwrap();
        assert_eq!(
            identity_of(&state, "acc-1"),
            (Some("credit_agricole".into()), Some("84890097587".into()))
        );
        assert_eq!(root_of(&state, "acc-1").as_deref(), Some("r-ca"));
    }

    /// An account with no root — one the phone made, say — joins the root of
    /// the source it is given, so the tree stays complete.
    #[tokio::test]
    async fn an_account_with_no_root_joins_the_root_of_its_source() {
        let state = state();
        account(&state, "acc-1", "Firstrade");
        assert_eq!(root_of(&state, "acc-1"), None);

        put(&state, "acc-1", Some("firstrade"), Some("abc123"))
            .await
            .unwrap();

        let root_id = root_of(&state, "acc-1").expect("it joined a root");
        let (name, source): (String, String) = state
            .db
            .get()
            .unwrap()
            .query_row(
                "SELECT name, source FROM account_roots WHERE id=?1",
                rusqlite::params![root_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(name, "Firstrade");
        assert_eq!(source, "firstrade");
    }

    /// Clearing the identity leaves the root alone: the account is still that
    /// institution's, it just has no provider number any more.
    #[tokio::test]
    async fn clearing_an_identity_leaves_the_root_alone() {
        let state = state();
        root(&state, "r-ca", "Crédit Agricole", "credit_agricole");
        leaf(&state, "acc-1", "Eko by CA", "credit_agricole", "r-ca");

        put(&state, "acc-1", None, None).await.unwrap();
        assert_eq!(identity_of(&state, "acc-1"), (None, None));
        assert_eq!(
            root_of(&state, "acc-1").as_deref(),
            Some("r-ca"),
            "the account is still Crédit Agricole's"
        );
    }

    /// The type is the server's to decide. The pull sends it down; a push that
    /// carries one the server does not know — a phone with the old vocabulary
    /// — is ignored rather than applied, so a sync cannot revert the rename.
    #[tokio::test]
    async fn a_push_cannot_revert_the_account_type() {
        let state = state();
        {
            let conn = state.db.get().unwrap();
            conn.execute(
                "INSERT INTO accounts (id, name, type, opening_balance, note, created_at, updated_at)
                 VALUES ('acc-1', 'Livret A', 'savings', 0.0, NULL, 1, 1)",
                [],
            )
            .unwrap();
        }

        let push: crate::handlers::push::BudgetAccountsPush = serde_json::from_value(
            serde_json::json!({
                "accounts": [{
                    "id": "acc-1",
                    "name": "Livret A",
                    "type": "bank",
                    "openingBalance": 0.0,
                    "note": null,
                    "createdAt": 1,
                    "deletedAt": null,
                }],
                "deleted": [],
            }),
        )
        .unwrap();
        crate::handlers::push::push_budget_accounts(State(state.clone()), Json(push)).await;

        let type_: String = state
            .db
            .get()
            .unwrap()
            .query_row("SELECT type FROM accounts WHERE id='acc-1'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(type_, "savings", "the stale vocabulary did not land");
    }

    /// The phone does not push the type at all, and the desktop does. A push
    /// without one leaves the account as it is; a push with one the server
    /// knows sets it.
    #[tokio::test]
    async fn a_push_sets_the_type_only_when_it_carries_one_the_server_knows() {
        let state = state();
        {
            let conn = state.db.get().unwrap();
            conn.execute(
                "INSERT INTO accounts (id, name, type, opening_balance, note, created_at, updated_at)
                 VALUES ('acc-1', 'Eko by CA', 'cash', 0.0, NULL, 1, 1)",
                [],
            )
            .unwrap();
        }

        // The phone's push: no type at all.
        let phone: crate::handlers::push::BudgetAccountsPush = serde_json::from_value(
            serde_json::json!({
                "accounts": [{
                    "id": "acc-1",
                    "name": "Eko by CA",
                    "openingBalance": 0.0,
                    "note": null,
                    "createdAt": 1,
                    "deletedAt": null,
                }],
                "deleted": [],
            }),
        )
        .unwrap();
        crate::handlers::push::push_budget_accounts(State(state.clone()), Json(phone)).await;

        let type_: String = state
            .db
            .get()
            .unwrap()
            .query_row("SELECT type FROM accounts WHERE id='acc-1'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(type_, "cash", "a push without a type leaves it alone");

        // The desktop's push: a type the server knows.
        let desktop: crate::handlers::push::BudgetAccountsPush = serde_json::from_value(
            serde_json::json!({
                "accounts": [{
                    "id": "acc-1",
                    "name": "Eko by CA",
                    "type": "savings",
                    "openingBalance": 0.0,
                    "note": null,
                    "createdAt": 1,
                    "deletedAt": null,
                }],
                "deleted": [],
            }),
        )
        .unwrap();
        crate::handlers::push::push_budget_accounts(State(state.clone()), Json(desktop)).await;

        let type_: String = state
            .db
            .get()
            .unwrap()
            .query_row("SELECT type FROM accounts WHERE id='acc-1'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(type_, "savings", "the desktop's edit landed");
    }

    /// A number a *deleted* account still holds is refused to everyone else —
    /// the partial unique index covers tombstones — so clearing it from the
    /// tombstone has to be possible, or the number is unusable for good and
    /// the account holding it is invisible on the accounts screen.
    #[tokio::test]
    async fn a_number_held_by_a_tombstone_can_be_freed() {
        let state = state();
        account(&state, "dead", "Old Firstrade");
        account(&state, "live", "Firstrade");
        put(&state, "dead", Some("firstrade"), Some("firstrade"))
            .await
            .unwrap();
        {
            let conn = state.db.get().unwrap();
            conn.execute("UPDATE accounts SET deleted_at=1 WHERE id='dead'", [])
                .unwrap();
        }

        let (status, Json(body)) = put(&state, "live", Some("firstrade"), Some("firstrade"))
            .await
            .unwrap_err();
        assert_eq!(status, StatusCode::CONFLICT);
        assert!(
            body.message.contains("deleted account"),
            "the refusal has to say the holder is a tombstone: {}",
            body.message
        );

        // Clearing works on a tombstone even though setting does not.
        put(&state, "dead", None, None).await.unwrap();
        put(&state, "live", Some("firstrade"), Some("firstrade"))
            .await
            .unwrap();
        assert_eq!(
            identity_of(&state, "live"),
            (Some("firstrade".into()), Some("firstrade".into()))
        );
    }

    #[tokio::test]
    async fn an_unknown_or_deleted_account_is_refused() {
        let state = state();
        let (status, _) = put(&state, "nope", Some("firstrade"), Some("x")).await.unwrap_err();
        assert_eq!(status, StatusCode::NOT_FOUND);

        account(&state, "acc-1", "Gone");
        {
            let conn = state.db.get().unwrap();
            conn.execute("UPDATE accounts SET deleted_at=1 WHERE id='acc-1'", [])
                .unwrap();
        }
        let (status, _) = put(&state, "acc-1", Some("firstrade"), Some("x")).await.unwrap_err();
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }
}
