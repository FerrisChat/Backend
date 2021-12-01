use crate::WebServerError;
use axum::extract::Path;
use ferrischat_common::types::NotFoundJson;
use serde::Serialize;

/// DELETE `/api/v0/users/{user_id}/bots/{bot_id}`
/// Deletes the bot
pub async fn delete_bot(
    Path((user_id, bot_id)): Path<(u128, u128)>,
    auth: crate::Authorization,
) -> Result<http::StatusCode, WebServerError<impl Serialize>> {
    let bigint_user_id = u128_to_bigdecimal!(user_id);

    let db = get_db_or_fail!();

    let owner_id = bigdecimal_to_u128!(
        sqlx::query!("SELECT * FROM bots WHERE user_id = $1", bigint_user_id)
            .fetch_optional(db)
            .await?
            .ok_or_else(|| {
                (
                    404,
                    NotFoundJson {
                        message: format!("Unknown bot with ID {}", bot_id),
                    },
                )
                    .into()
            })?
            .owner_id
    );

    if owner_id != auth.0 {
        return Err((
            403,
            ferrischat_common::types::Json {
                message: "you are not the owner of this bot".to_string(),
            },
        )
            .into());
    }

    sqlx::query!("DELETE FROM users WHERE id = $1", bigint_user_id)
        .execute(db)
        .await?;

    Ok(http::StatusCode::NO_CONTENT)
}
