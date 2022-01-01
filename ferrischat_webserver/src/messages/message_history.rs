use crate::WebServerError;
use axum::extract::{Path, Query};
use ferrischat_common::request_json::GetMessageHistoryParams;
use ferrischat_common::types::{Channel, ErrorJson, Message, MessageHistory, User, UserFlags};

/// GET `/v0/channels/{channel_id}/messages`
pub async fn get_message_history(
    Path(channel_id): Path<u128>,
    crate::Authorization(_, _): crate::Authorization,
    Query(GetMessageHistoryParams {
        limit,
        oldest_first,
        mut offset,
    }): Query<GetMessageHistoryParams>,
) -> Result<crate::Json<MessageHistory>, WebServerError> {
    let bigdecimal_channel_id = u128_to_bigdecimal!(channel_id);
    let db = get_db_or_fail!();

    let oldest_first = oldest_first.unwrap_or(false);

    if limit < Some(0) {
        return Err(ErrorJson::new_400("limit must be > 0".to_string()).into());
    }

    if offset < Some(0) {
        offset = Some(0);
    }

    let channel = sqlx::query!("SELECT * FROM channels WHERE id = $1", bigdecimal_channel_id)
        .fetch_optional(db)
        .await?
        .ok_or_else(|| ErrorJson::new_404("channel not found".to_string()))?;

    let channel_obj = Channel {
        id: channel_id,
        name: channel.name,
        guild_id: bigdecimal_to_u128!(channel.guild_id),
    };

    let messages: Vec<_> = if oldest_first {
        let resp = sqlx::query!(
            r#"
SELECT m.*,
       a.avatar AS avatar,
       a.name AS author_name,
       a.flags AS author_flags,
       a.discriminator AS author_discriminator,
       a.pronouns AS author_pronouns
FROM messages m
    CROSS JOIN LATERAL (
        SELECT *
        FROM users
        WHERE id = m.author_id
        ) as a
WHERE channel_id = $1
ORDER BY id ASC
LIMIT $2 OFFSET $3
"#,
            bigdecimal_channel_id,
            limit,
            offset,
        )
        .fetch_all(db)
        .await?;

        resp.into_iter()
            .map(|x| {
                (
                    x.id,
                    x.content,
                    x.channel_id,
                    x.avatar,
                    x.author_id,
                    x.author_name,
                    x.author_flags,
                    x.author_discriminator,
                    x.author_pronouns,
                    x.edited_at,
                )
            })
            .collect()
    } else {
        let resp = sqlx::query!(
            r#"
SELECT m.*,
       a.name AS author_name,
       a.avatar AS avatar,
       a.flags AS author_flags,
       a.discriminator AS author_discriminator,
       a.pronouns AS author_pronouns
FROM messages m
    CROSS JOIN LATERAL (
        SELECT *
        FROM users 
        WHERE id = m.author_id
        ) as a
WHERE channel_id = $1
ORDER BY id DESC
LIMIT $2 OFFSET $3
"#,
            bigdecimal_channel_id,
            limit,
            offset,
        )
        .fetch_all(db)
        .await?;

        resp.into_iter()
            .map(|x| {
                (
                    x.id,
                    x.content,
                    x.channel_id,
                    x.avatar,
                    x.author_id,
                    x.author_name,
                    x.author_flags,
                    x.author_discriminator,
                    x.author_pronouns,
                    x.edited_at,
                )
            })
            .collect()
    };

    let mut output_messages = Vec::with_capacity(messages.len());
    for (
        id,
        content,
        channel_id,
        avatar,
        author_id,
        author_name,
        author_flags,
        author_discriminator,
        author_pronouns,
        edited_at,
    ) in messages
    {
        let author_id = bigdecimal_to_u128!(author_id);
        let id = bigdecimal_to_u128!(id);
        let channel_id = bigdecimal_to_u128!(channel_id);

        output_messages.push(Message {
            id,
            content,
            channel: channel_obj.clone(),
            channel_id,
            author_id,
            author: Some(User {
                id: author_id,
                name: author_name,
                avatar,
                guilds: None,
                flags: UserFlags::from_bits_truncate(author_flags),
                discriminator: author_discriminator,
                pronouns: author_pronouns.and_then(ferrischat_common::types::Pronouns::from_i16),
                is_bot: {
                    UserFlags::from_bits_truncate(author_flags).contains(UserFlags::BOT_ACCOUNT)
                },
            }),
            edited_at,
            embeds: vec![],
            nonce: None,
        });
    }

    Ok(crate::Json {
        obj: MessageHistory {
            messages: output_messages,
        },
        code: 200,
    })
}
