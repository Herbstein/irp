use std::sync::Arc;

use futures_util::StreamExt;
use irp_proto::irp::{
    CarTelemetry, FooBarRequest, Handshake, Telemetry, foo_bar_request, irp_client::IrpClient,
};
use tokio::sync::{Mutex, mpsc::UnboundedReceiver};

use crate::IrpMessage;

pub async fn connect(rx: UnboundedReceiver<IrpMessage>) {
    let rx = Arc::new(Mutex::new(rx));

    loop {
        let mut client = match IrpClient::connect("http://localhost:50051").await {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Failed to connect to IRP server: {}", e);
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                continue;
            }
        };
        let rx = rx.clone();

        let outbound = async_stream::stream! {
            let mut has_sent_handshake = false;

            while let Some(message) = rx.lock().await.recv().await {
                match message {
                    IrpMessage::Telemetry(data) => {
                        if !has_sent_handshake {
                            continue;
                        }

                        if !data.is_on_track {
                            continue;
                        }

                        let cars = data
                            .car_idx_lap_dist_pct
                            .into_iter()
                            .map(|lap_dist_pct| CarTelemetry {
                                lap_pct: lap_dist_pct,
                            })
                            .collect();

                        yield FooBarRequest {
                            msg: Some(foo_bar_request::Msg::Telemetry(Telemetry { cars })),
                        };
                    }
                    IrpMessage::SessionInfo => {
                        if !has_sent_handshake {
                            yield FooBarRequest {
                                msg: Some(foo_bar_request::Msg::Handshake(Handshake {custid: 0, subsessionid: 0}))
                            };
                            has_sent_handshake = true;
                        }
                    }
                    IrpMessage::LapSummary => {
                        println!("Lap summary received");
                    }
                }
            }
        };

        let Ok(response) = client.foo_bar(outbound).await else {
            eprintln!("Failed to call IRP server");
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            continue;
        };

        let mut inbound = response.into_inner();

        while inbound.next().await.is_some() {}
    }
}
