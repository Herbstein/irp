use futures_util::StreamExt;
use irp_proto::{
    irp,
    irp::{foo_bar_request, irp_client::IrpClient},
};

use crate::State;

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

        let outbound = async_stream::stream! {
            let mut was_connected = false;

            loop {
                state.notify.notified().await;

                let (session_info, summaries, telemetry, connected) = {
                    let mut daemon_state = state.state.lock().expect("Poisoned state lock");

                    let session_info = daemon_state.latest_session_info.take();
                    let summaries = std::mem::take(&mut daemon_state.pending_lap_summaries);
                    let telemetry = daemon_state.latest_telemetry.take();
                    let connected = daemon_state.sim_connected;

                    (session_info, summaries, telemetry, connected)
                };

                if !connected {
                    // Don't do more processing if we're not connected to the sim
                    was_connected = false;
                    continue;
                }

                if !was_connected && connected {
                    match session_info {
                        Some(session_info) => {
                            was_connected = true;

                            let subsessionid = session_info.weekend_info.sub_session_id;
                            let custid = session_info.driver_info.drivers[session_info.driver_info.driver_car_idx as usize].user_id;

                            yield irp::FooBarRequest {
                                msg: Some(foo_bar_request::Msg::Handshake(irp::Handshake {
                                    custid: custid as u32,
                                    subsessionid: subsessionid as u64,
                                }))
                            }
                        }
                        None => continue, // Wait until session info is present
                    }
                }

                for summary in summaries {
                    yield irp::FooBarRequest {
                        msg: Some(foo_bar_request::Msg::Summary(irp::LapSummary {summaries: vec![]}))
                    }
                }

                if let Some(data) = telemetry {
                    let cars = data
                        .car_idx_lap_dist_pct
                        .into_iter()
                        .map(|lap_dist_pct| irp::CarTelemetry {
                            lap_pct: lap_dist_pct,
                        })
                        .collect();

                    yield irp::FooBarRequest {
                        msg: Some(foo_bar_request::Msg::Telemetry(irp::Telemetry { cars }))
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
