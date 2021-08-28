use actix_web::{web::Json, HttpResponse, Responder};
use ferrischat_common::request_json::UserCreateJson;
use ferrischat_common::types::{InternalServerErrorJson, ModelType, User};
use ferrischat_snowflake_generator::generate_snowflake;
use tokio::sync::oneshot::channel;

/// POST /api/v0/users/
pub async fn create_user(user_data: Json<UserCreateJson>) -> impl Responder {
    let db = get_db_or_fail!();
    let user_id = generate_snowflake::<0>(ModelType::User as u8, 0);
    let UserCreateJson {
        username,
        email,
        password,
    } = user_data.0;

    let hashed_password = {
        let hasher = match crate::GLOBAL_HASHER.get() {
            Some(h) => h,
            None => {
                return HttpResponse::InternalServerError().json(InternalServerErrorJson {
                    reason: "Password hasher not found".to_string(),
                })
            }
        };
        let (tx, rx) = channel();
        // if this fn errors it will be caught because tx will be dropped as well
        // resulting in a error in the match
        let _ = hasher.send((password, tx)).await;
        match rx.await {
            Ok(d) => match d {
                Ok(s) => s,
                Err(e) => {
                    return HttpResponse::InternalServerError().json(InternalServerErrorJson {
                        reason: format!("Failed to hash password: {}", e),
                    })
                }
            },
            Err(_) => {
                return HttpResponse::InternalServerError().json(InternalServerErrorJson {
                    reason: "Other end hung up connection".to_string(),
                })
            }
        }
    };

    let bigint_user_id = u128_to_bigdecimal!(user_id);
    match sqlx::query!(
        "INSERT INTO users VALUES ($1, $2, $3, $4, $5)",
        bigint_user_id,
        username,
        0,
        email,
        hashed_password,
    )
    .execute(db)
    .await
    {
        Ok(_) => HttpResponse::Created().json(User {
            id: user_id,
            name: username,
            avatar: None,
            guilds: None,
            flags: 0,
        }),
        Err(e) => HttpResponse::InternalServerError().json(InternalServerErrorJson {
            reason: format!("DB returned a error: {}", e),
        }),
    }
}
