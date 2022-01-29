use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::error_handling::handle_error;
#[allow(clippy::wildcard_imports)]
use crate::{error_handling::WsEventHandlerError, events::*, USERID_CONNECTION_MAP};
use ferrischat_common::ws::{WsInboundEvent, WsOutboundEvent};
use ferrischat_redis::REDIS_MANAGER;
use futures_util::stream::SplitStream;
use futures_util::StreamExt;
use tokio::net::UnixStream;
use tokio_tungstenite::tungstenite::error::Error;
use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;
use tokio_tungstenite::tungstenite::protocol::CloseFrame;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::WebSocketStream;
use uuid::Uuid;

fn decode_event<'a>(
    msg: Result<Message, Error>,
) -> Result<Option<WsInboundEvent>, Option<CloseFrame<'a>>> {
    match msg {
        Ok(Message::Text(t)) => {
            debug!("got text payload");
            Ok(Some(
                match simd_json::serde::from_slice(&mut t.into_bytes()[..]) {
                    Ok(d) => d,
                    Err(e) => {
                        return Err(Some(CloseFrame {
                            code: CloseCode::from(2001),
                            reason: format!("invalid JSON found: {}", e).into(),
                        }));
                    }
                },
            ))
        }
        Ok(Message::Binary(_)) => {
            debug!("got binary payload");
            Err(Some(CloseFrame {
                code: CloseCode::Unsupported,
                reason: "Binary data sent: only text supported at the moment".into(),
            }))
        }
        Ok(Message::Ping(_) | Message::Pong(_)) => {
            debug!("got ping/pong payload");
            Ok(None)
        }
        Ok(Message::Close(_)) => Err(None),
        Err(e) => return Err(Some(handle_error(e))),
    }
}

pub async fn rx_handler(
    mut rx: SplitStream<WebSocketStream<UnixStream>>,
    inter_tx: tokio::sync::mpsc::Sender<WsOutboundEvent>,
    closer_tx: futures::channel::oneshot::Sender<Option<CloseFrame<'_>>>,
    conn_id: Uuid,
) -> SplitStream<WebSocketStream<UnixStream>> {
    let identify_received = Arc::new(AtomicBool::new(false));

    let _redis_conn = if let Some(r) = REDIS_MANAGER.get() {
        r.clone()
    } else {
        let _tx = closer_tx.send(Some(CloseFrame {
            code: CloseCode::from(5002),
            reason: "Redis pool not found".into(),
        }));
        return rx;
    };
    let db = if let Some(db) = ferrischat_db::DATABASE_POOL.get() {
        db
    } else {
        let _tx = closer_tx.send(Some(CloseFrame {
            code: CloseCode::from(5003),
            reason: "Database pool not found".into(),
        }));
        return rx;
    };

    let uid_conn_map = if let Some(m) = USERID_CONNECTION_MAP.get() {
        m
    } else {
        let _tx = closer_tx.send(Some(CloseFrame {
            code: CloseCode::from(5004),
            reason: "Connection map not found".into(),
        }));
        return rx;
    };

    while let Some(item) = rx.next().await {
        let data = match decode_event(item) {
            Ok(Some(e)) => e,
            Ok(None) => continue,
            Err(e) => {
                let _tx = closer_tx.send(e);
                break;
            }
        };

        if !identify_received.load(Ordering::Relaxed) {
            if let WsInboundEvent::Identify { .. } = data {
            } else {
                let _tx = closer_tx.send(Some(CloseFrame {
                    code: CloseCode::from(2004),
                    reason: "data payload sent before identifying".into(),
                }));
                break;
            }
        }

        let handler_data = RxHandlerData {
            inter_tx: inter_tx.clone(),
            uid_conn_map,
            identify_received: Arc::clone(&identify_received),
        };

        let handler_response = match data {
            WsInboundEvent::Identify { token, intents } => {
                IdentifyEvent::handle_event(
                    db,
                    RxEventData::Identify { token, intents },
                    handler_data,
                    conn_id,
                )
                .await
            }
            WsInboundEvent::Ping => {
                PingEvent::handle_event(db, RxEventData::Ping, handler_data, conn_id).await
            }
            WsInboundEvent::Pong => {
                PongEvent::handle_event(db, RxEventData::Pong, handler_data, conn_id).await
            }
        };
        match handler_response {
            Err(WsEventHandlerError::Sender) => break,
            Err(WsEventHandlerError::CloseFrame(f)) => {
                // either way we're breaking out, and a error here just means the other end hung
                // up already, and has already returned, meaning it's just waiting on us to return
                let _tx = closer_tx.send(Some(f));
                break;
            }
            _ => {}
        }
    }
    rx
}
