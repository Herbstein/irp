use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use futures_util::StreamExt;
use irp_proto::{
    irp,
    irp::{foo_bar_request, irp_client::IrpClient},
};

use crate::{LiveData, SessionInfo, State};

enum Phase {
    WaitingForSim,
    WaitingForAck,
    Streaming { sent_session_info: bool },
}

pub async fn connect(state: State) {
    loop {
        let mut client = match IrpClient::connect("http://localhost:50051").await {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Failed to connect to IRP server: {}", e);
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                continue;
            }
        };

        let state = state.clone();
        let acked = Arc::new(AtomicBool::new(false));
        let acked_clone = acked.clone();

        let outbound = async_stream::stream! {
            let mut phase = Phase::WaitingForSim;

            let acked = acked_clone;

            loop {
                state.notify.notified().await;

                // Take snapshot of the daemon state
                let (session_info, telemetry, connected) = {
                    let mut daemon_state = state.state.lock().expect("Poisoned state lock");

                    let session_info = daemon_state.session_info.clone();
                    let telemetry = daemon_state.latest_telemetry.take();
                    let connected = daemon_state.sim_connected;

                    (session_info, telemetry, connected)
                };

                match &mut phase {
                    Phase::WaitingForSim => {
                        if !connected {
                            continue;
                        }
                        let Some(info) = session_info.session_info() else { continue };
                        yield handshake(info);
                        phase = Phase::WaitingForAck;
                    }
                    Phase::WaitingForAck => {
                        if !acked.load(Ordering::Acquire) {
                            continue;
                        }
                        phase = Phase::Streaming { sent_session_info: false };
                    }
                    Phase::Streaming { sent_session_info } => {
                        if !connected {
                            phase = Phase::WaitingForSim;
                            continue;
                        }
                        if let Some(data) = telemetry {
                            yield telemetry_frame(data);
                        }
                        if (!*sent_session_info || session_info.is_new()) && let Some(session_info) = session_info.session_info() {
                            *sent_session_info = true;
                            yield session_info_msg(session_info);
                        }
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

        while let Some(msg) = inbound.next().await {
            if let Ok(irp::FooBarResponse {
                msg: Some(irp::foo_bar_response::Msg::HandshakeAck(_)),
            }) = msg
            {
                acked.store(true, Ordering::Release);
            }
        }

        println!("Connection closed, retrying...");
    }
}

fn handshake(session_info: &SessionInfo) -> irp::FooBarRequest {
    let hero_car_idx = session_info.driver_info.driver_car_idx as usize;

    let subsessionid = session_info.weekend_info.sub_session_id;
    let custid = session_info.driver_info.drivers[hero_car_idx].user_id;

    irp::FooBarRequest {
        msg: Some(foo_bar_request::Msg::Handshake(irp::Handshake {
            custid,
            subsessionid,
        })),
    }
}

fn telemetry_frame(data: LiveData) -> irp::FooBarRequest {
    let mut telem_data = Vec::with_capacity(data.car_idx_lap_dist_pct.len());

    for car_idx in 0..data.car_idx_lap_dist_pct.len() {
        let lap_dist_pct = data.car_idx_lap_dist_pct[car_idx];

        telem_data.push(irp::CarTelemetry {
            lap_pct: lap_dist_pct,
        });
    }

    let hero = irp::HeroTelemetry {
        fuel_level: data.fuel_level,
        fuel_level_pct: data.fuel_level_pct,
    };

    irp::FooBarRequest {
        msg: Some(foo_bar_request::Msg::Telemetry(irp::Telemetry {
            cars: telem_data,
            hero: Some(hero),
        })),
    }
}

fn session_info_msg(session_info: &SessionInfo) -> irp::FooBarRequest {
    irp::FooBarRequest {
        msg: Some(foo_bar_request::Msg::SessionInfo(irp::SessionInfo {
            weekend_info: Some(irp::WeekendInfo {
                track_id: session_info.weekend_info.track_id,
                sub_session_id: session_info.weekend_info.sub_session_id,
            }),
            driver_info: Some(irp::DriverInfo {
                driver_car_idx: session_info.driver_info.driver_car_idx,
                drivers: session_info
                    .driver_info
                    .drivers
                    .iter()
                    .map(|d| irp::Driver {
                        car_idx: d.car_idx,
                        user_name: d.user_name.clone(),
                    })
                    .collect(),
            }),
        })),
    }
}
