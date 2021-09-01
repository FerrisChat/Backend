use actix_web::{web::Query, HttpRequest, HttpResponse, Responder};

use ferrischat_common::request_json::GetMessageHistoryParams;
use ferrischat_common::types::{BadRequestJson, InternalServerErrorJson, Message, MessageHistory};

/// GET /api/v0/channels/{channel_id}/messages
pub async fn get_message_history(
    req: HttpRequest,
    _: crate::Authorization,
    params: Query<GetMessageHistoryParams>,
) -> impl Responder {
    let channel_id = get_item_id!(req, "channel_id");
    let bigint_channel_id = u128_to_bigdecimal!(channel_id);
    let db = get_db_or_fail!();

    let mut limit = params.limit;

    if limit >= Some(9223372036854775807) {
        limit = None;
    }

    if limit < Some(0) {
        return HttpResponse::BadRequest().json(BadRequestJson {
            reason: "limit must be > 0".to_string(),
            location: None,
        });
    }

    let messages = {
        let resp = sqlx::query!(
            "SELECT * FROM messages WHERE channel_id = $1 LIMIT $2",
            bigint_channel_id,
            limit
        )
        .fetch_all(db)
        .await;
        match resp {
            Ok(resp) => resp
                .iter()
                .filter_map(|x| {
                    Some(Message {
                        id: x.id.with_scale(0).into_bigint_and_exponent().0.to_u128()?,
                        content: x.content.clone(),
                        channel_id: x
                            .channel_id
                            .with_scale(0)
                            .into_bigint_and_exponent()
                            .0
                            .to_u128()?,
                        author_id: x
                            .author_id
                            .with_scale(0)
                            .into_bigint_and_exponent()
                            .0
                            .to_u128()?,
                        edited_at: x.edited_at,
                    })
                })
                .collect(),
            Err(e) => {
                return HttpResponse::InternalServerError().json(InternalServerErrorJson {
                    reason: format!("database returned a error: {}", e),
                })
            }
        }
    };

    HttpResponse::Ok().json(MessageHistory { messages })
}
