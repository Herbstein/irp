use std::pin::Pin;

use async_stream::try_stream;
use axum::{
    Extension, Json, Router,
    extract::{
        WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    response::IntoResponse,
    routing::{any, get},
};
use futures_util::{SinkExt, Stream, StreamExt};
use irp_proto::irp::{
    FooBarRequest, FooBarResponse, HandshakeAck, foo_bar_request, foo_bar_response,
    irp_server::{Irp, IrpServer},
};
use serde::Serialize;
use tokio::{net::TcpListener, sync::broadcast};
use tonic::{async_trait, transport::Server};
use tower_http::services::ServeDir;

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

#[derive(Debug)]
struct IrpService {
    sender: broadcast::Sender<Telemetry>,
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
        let sender = self.sender.clone();

        let output = try_stream! {
            let mut hero_car_idx = -1;
            let mut drivers = Vec::new();

            while let Some(request) = stream.next().await {
                let request = request?;
                match request.msg {
                    Some(foo_bar_request::Msg::Handshake(handshake)) => {
                        println!("Handshake received: custid={}, subsessionid={}", handshake.custid, handshake.subsessionid);
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

                        let hero = match telemetry.hero {
                            Some(hero) => hero,
                            None => continue,
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

#[derive(Clone)]
struct ReceiverProvider {
    sender: broadcast::Sender<Telemetry>,
}

impl ReceiverProvider {
    pub fn get_receiver(&self) -> broadcast::Receiver<Telemetry> {
        self.sender.subscribe()
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let grpc_addr = "127.0.0.1:50051".parse()?;

    let (telemetry_tx, _) = broadcast::channel(10);

    let irp = IrpService {
        sender: telemetry_tx.clone(),
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
            .route("/api/next", get(next_handler))
            .layer(Extension(ReceiverProvider {
                sender: telemetry_tx,
            }))
            .fallback_service(ServeDir::new("dist/"));
        let listener = TcpListener::bind("127.0.0.1:8080").await?;
        axum::serve(listener, app).await
    });

    let (grpc_result, http_result) = tokio::try_join!(grpc, http)?;

    grpc_result?;
    http_result?;

    Ok(())
}

async fn next_handler(Extension(provider): Extension<ReceiverProvider>) -> Json<Telemetry> {
    let mut receiver = provider.get_receiver();
    let telemetry = receiver.recv().await.unwrap();
    Json(telemetry)
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    Extension(provider): Extension<ReceiverProvider>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, provider))
}

async fn handle_socket(socket: WebSocket, provider: ReceiverProvider) {
    let (mut sender, _) = socket.split();

    let mut receiver = provider.get_receiver();

    while let Ok(telemetry) = receiver.recv().await {
        if sender
            .send(Message::Text(
                serde_json::to_string(&telemetry).unwrap().into(),
            ))
            .await
            .is_err()
        {
            break;
        }
    }
}
