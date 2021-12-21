use crate::ws::fire_event;
use crate::WebServerError;
use axum::extract::Path;
use ferrischat_common::types::{ErrorJson, Guild, GuildFlags, Member};
use ferrischat_common::ws::WsOutboundEvent;

/// DELETE `/v0/guilds/{guild_id}`
pub async fn delete_guild(
    Path(guild_id): Path<u128>,
    auth: crate::Authorization,
) -> Result<http::StatusCode, WebServerError> {
    let db = get_db_or_fail!();
    let bigint_guild_id = u128_to_bigdecimal!(guild_id);

    let x = sqlx::query!("SELECT owner_id FROM guilds WHERE id = $1", bigint_guild_id)
        .fetch_optional(db)
        .await?
        .ok_or_else(|| ErrorJson::new_404(format!("Unknown guild with ID {}", guild_id)))?;
    let owner_id = bigdecimal_to_u128!(x.owner_id);
    if auth.0 != owner_id {
        return Err(ErrorJson::new_403("Forbidden".to_string()).into());
    }

    let guild_resp = sqlx::query!(
        "DELETE FROM guilds WHERE id = $1 RETURNING *",
        bigint_guild_id,
    )
    .fetch_one(db)
    .await?;
    let guild_obj = Guild {
        id: guild_id,
        owner_id: auth.0,
        name: guild_resp.name,
        channels: None,
        flags: GuildFlags::empty(),
        members: Some(vec![Member {
            guild_id: Some(guild_id),
            user_id: Some(auth.0),
            user: None,
            guild: None,
        }]),
        roles: None,
        avatar: guild_resp.avatar,
    };

    let event = WsOutboundEvent::GuildDelete {
        guild: guild_obj.clone(),
    };

    fire_event(&event).await?;
    Ok(http::StatusCode::NO_CONTENT)
}
