//! Extracted channel handlers from ws_multiplexed.

use super::*;

pub(super) async fn handle_bus_command(
    cmd: crate::bus::BusCommand,
    user_id: &str,
    is_admin: bool,
    state: &AppState,
    conn_state: Arc<tokio::sync::Mutex<WsConnectionState>>,
) -> Option<WsEvent> {
    use crate::bus::{BusCommand, BusEvent, BusWsEvent, EventSource};

    let bus_sub_id = {
        let cs = conn_state.lock().await;
        cs.bus_subscriber_id
    };

    match cmd {
        BusCommand::Publish {
            id,
            scope,
            scope_id,
            topic,
            payload,
            v,
            priority,
            ttl_ms,
            idempotency_key,
            correlation_id,
            ack,
        } => {
            let mut event = BusEvent::new(
                scope,
                scope_id,
                topic,
                payload,
                EventSource::Frontend {
                    user_id: user_id.to_string(),
                    session_id: None,
                },
            );
            event.v = v;
            event.priority = priority;
            event.ttl_ms = ttl_ms;
            event.idempotency_key = idempotency_key;
            event.correlation_id = correlation_id;
            event.ack = ack;

            match state.bus.publish(Some(bus_sub_id), event).await {
                Ok(()) => Some(WsEvent::Bus(BusWsEvent::Response {
                    id,
                    success: true,
                    error: None,
                    data: None,
                })),
                Err(e) => Some(WsEvent::Bus(BusWsEvent::Response {
                    id,
                    success: false,
                    error: Some(e),
                    data: None,
                })),
            }
        }
        BusCommand::Subscribe {
            id,
            topics,
            scope,
            scope_id,
            filter,
        } => {
            match state
                .bus
                .subscribe(
                    bus_sub_id, user_id, is_admin, scope, scope_id, topics, filter,
                )
                .await
            {
                Ok(()) => Some(WsEvent::Bus(BusWsEvent::Response {
                    id,
                    success: true,
                    error: None,
                    data: None,
                })),
                Err(e) => Some(WsEvent::Bus(BusWsEvent::Response {
                    id,
                    success: false,
                    error: Some(e),
                    data: None,
                })),
            }
        }
        BusCommand::Unsubscribe {
            id,
            topics,
            scope,
            scope_id,
        } => {
            state
                .bus
                .unsubscribe(bus_sub_id, &scope, &scope_id, &topics);
            Some(WsEvent::Bus(BusWsEvent::Response {
                id,
                success: true,
                error: None,
                data: None,
            }))
        }
        BusCommand::Pull {
            id,
            topics,
            scope,
            scope_id,
            since_ts,
            limit,
        } => match state
            .bus
            .pull_for_user(user_id, is_admin, scope, scope_id, topics, since_ts, limit)
            .await
        {
            Ok(events) => Some(WsEvent::Bus(BusWsEvent::Response {
                id,
                success: true,
                error: None,
                data: Some(serde_json::json!({ "events": events })),
            })),
            Err(e) => Some(WsEvent::Bus(BusWsEvent::Response {
                id,
                success: false,
                error: Some(e),
                data: None,
            })),
        },
    }
}
