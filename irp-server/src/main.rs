use std::pin::Pin;

use async_stream::try_stream;
use axum::{
    Extension, Router,
    extract::{
        WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    response::IntoResponse,
    routing::any,
};
use futures_util::{SinkExt, Stream, StreamExt, TryStreamExt};
use irp_proto::irp::{
    FooBarRequest, FooBarResponse, HandshakeAck, foo_bar_request, foo_bar_response,
    irp_server::{Irp, IrpServer},
};
use serde::{Deserialize, Serialize};
use tokio::{net::TcpListener, sync::broadcast};
use tonic::{async_trait, transport::Server};
use tower_http::services::ServeDir;

use crate::registry::DaemonRegistry;

mod registry;

#[derive(Clone, Serialize)]
struct CarTelemetry {
    lap_pct: f32,
}

#[derive(Clone, Serialize)]
struct HeroTelemetry {
    fuel_level: f32,
    fuel_level_pct: f32,
}

#[derive(Clone, Serialize)]
struct Telemetry {
    cars: Vec<CarTelemetry>,
    hero: HeroTelemetry,
    hero_car_idx: i32,
}

struct Driver {
    car_idx: i32,
    user_name: String,
}

struct IrpService {
    daemon_registry: DaemonRegistry,
}

#[async_trait]
impl Irp for IrpService {
    type FooBarStream =
        Pin<Box<dyn Stream<Item = Result<FooBarResponse, tonic::Status>> + Send + Sync>>;

    async fn foo_bar(
        &self,
        request: tonic::Request<tonic::codec::Streaming<FooBarRequest>>,
    ) -> Result<tonic::Response<Self::FooBarStream>, tonic::Status> {
        let mut stream = request.into_inner();

        let registry = self.daemon_registry.clone();

        let output = try_stream! {
            let mut hero_car_idx = -1;
            let mut drivers = Vec::new();

            let mut sender = None;

            while let Some(request) = stream.next().await {
                let request = request?;
                match request.msg {
                    Some(foo_bar_request::Msg::Handshake(handshake)) => {
                        println!("Handshake received: custid={}, subsessionid={}", handshake.custid, handshake.subsessionid);
                        sender = Some(registry.register(handshake.custid));
                        let response = FooBarResponse {
                            msg: Some(foo_bar_response::Msg::HandshakeAck(HandshakeAck {}))
                        };
                        yield response;
                    }
                    Some(foo_bar_request::Msg::Telemetry(telemetry)) => {
                        // We don't know the hero car idx yet, so ignore telemetry until we do. This should arrive very early
                        if hero_car_idx == -1 {
                            println!("Ignoring telemetry until hero car idx is known");
                            continue;
                        }

                        let Some(hero) = telemetry.hero else {
                            continue;
                        };

                        let Some(sender) = &sender else {
                            continue;
                        };

                        sender
                            .send(Telemetry {
                                cars: telemetry
                                    .cars
                                    .into_iter()
                                    .map(|c| CarTelemetry { lap_pct: c.lap_pct })
                                    .collect(),
                                hero: HeroTelemetry {
                                    fuel_level: hero.fuel_level,
                                    fuel_level_pct: hero.fuel_level_pct,
                                },
                                hero_car_idx,
                            })
                            .ok();
                    }
                    Some(foo_bar_request::Msg::SessionInfo(session_info)) => {
                        if let Some(driver_info) = session_info.driver_info {
                            hero_car_idx = driver_info.driver_car_idx;
                            drivers = driver_info.drivers.into_iter().map(|d| Driver { car_idx: d.car_idx, user_name: d.user_name }).collect();
                            drivers.sort_unstable_by_key(|d| d.car_idx);
                        }
                    }
                    None => continue,
                }
            }
        };

        Ok(tonic::Response::new(Box::pin(output) as Self::FooBarStream))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let grpc_addr = "127.0.0.1:50051".parse()?;

    let daemon_registry = DaemonRegistry::new();

    let irp = IrpService {
        daemon_registry: daemon_registry.clone(),
    };

    let service = IrpServer::new(irp);

    let grpc = tokio::spawn(async move {
        Server::builder()
            .add_service(service)
            .serve(grpc_addr)
            .await
    });

    let http = tokio::spawn(async move {
        let app = Router::new()
            .route("/ws", any(ws_handler))
            .layer(Extension(daemon_registry))
            .fallback_service(ServeDir::new("dist/"));
        let listener = TcpListener::bind("127.0.0.1:8080").await?;
        axum::serve(listener, app).await
    });

    let (grpc_result, http_result) = tokio::try_join!(grpc, http)?;

    grpc_result?;
    http_result?;

    Ok(())
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    Extension(registry): Extension<DaemonRegistry>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, registry))
}

#[derive(Serialize)]
#[serde(tag = "type", content = "data")]
enum WsMessage {
    #[serde(rename = "list_daemons")]
    Daemons { daemons: Vec<i32> },
    #[serde(rename = "telemetry")]
    Telemetry { telemetry: Telemetry },
}

#[derive(Deserialize)]
#[serde(tag = "type", content = "data")]
enum WsRequest {
    #[serde(rename = "select_daemon")]
    SelectDaemon(i32),
}

enum WsState {
    WaitingForSelection,
    Listening {
        receiver: broadcast::Receiver<Telemetry>,
    },
}

async fn handle_socket(socket: WebSocket, registry: DaemonRegistry) {
    let (mut ws_sender, ws_receiver) = socket.split();

    let ws_receiver = ws_receiver
        .map_err(|_| ())
        .try_filter_map(async |msg| match msg {
            Message::Text(text) => {
                let request = serde_json::from_str::<WsRequest>(&text).map_err(|_| ())?;
                Ok(Some(request))
            }
            _ => Err(()),
        });
    let mut ws_receiver = Box::pin(ws_receiver);

    let mut registry_receiver = registry.subscribe();

    let msg = serde_json::to_string(&WsMessage::Daemons {
        daemons: registry_receiver.borrow_and_update().clone(),
    })
    .unwrap();
    ws_sender.send(Message::Text(msg.into())).await.unwrap();

    let mut state = WsState::WaitingForSelection;

    loop {
        match &mut state {
            WsState::WaitingForSelection => {
                tokio::select! {
                    request = ws_receiver.next() => {
                        match request {
                            Some(Ok(WsRequest::SelectDaemon(custid))) => {
                                let new_receiver = registry.receiver(custid);
                                match new_receiver {
                                    Some(receiver) => {
                                        state = WsState::Listening { receiver };
                                    }
                                    None => {
                                        // TODO(herbstein): Respond with an error
                                    }
                                }
                            }
                            _ => break,
                        }
                    }
                    result = registry_receiver.changed() => {
                        if result.is_err() {
                            break;
                        }
                        let daemons = registry_receiver.borrow_and_update().clone();
                        let msg = serde_json::to_string(&WsMessage::Daemons { daemons }).unwrap();
                        if ws_sender.send(Message::Text(msg.into())).await.is_err() {
                            break;
                        }
                    }
                }
            }
            WsState::Listening { receiver } => {
                tokio::select! {
                    result = receiver.recv() => {
                        match result {
                            Ok(telemetry) => {
                                let msg = serde_json::to_string(&WsMessage::Telemetry { telemetry }).unwrap();
                                if ws_sender.send(Message::Text(msg.into())).await.is_err() {
                                    break;
                                }
                            }
                            Err(broadcast::error::RecvError::Lagged(_)) => {}
                            Err(broadcast::error::RecvError::Closed) => {
                                // Daemon disconnected, go back to selection
                                state = WsState::WaitingForSelection;
                            }
                        }
                    }
                    request = ws_receiver.next() => {
                        match request {
                            Some(Ok(WsRequest::SelectDaemon(custid))) => {
                                if let Some(new_receiver) = registry.receiver(custid) {
                                    state = WsState::Listening { receiver: new_receiver };
                                }
                            }
                            _ => break,
                        }
                    }
                    result = registry_receiver.changed() => {
                        if result.is_err() {
                            break;
                        }
                        let daemons = registry_receiver.borrow_and_update().clone();
                        let msg = serde_json::to_string(&WsMessage::Daemons { daemons }).unwrap();
                        if ws_sender.send(Message::Text(msg.into())).await.is_err() {
                            break;
                        }
                    }
                }
            }
        }
    }
}
