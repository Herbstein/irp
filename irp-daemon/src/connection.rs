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
            let mut hero_car_idx = 0;

            loop {
                state.notify.notified().await;

                let (session_info, telemetry, connected) = {
                    let mut daemon_state = state.state.lock().expect("Poisoned state lock");

                    let session_info = daemon_state.latest_session_info.clone();
                    let telemetry = daemon_state.latest_telemetry.take();
                    let connected = daemon_state.sim_connected;

                    (session_info, telemetry, connected)
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

                            hero_car_idx = session_info.driver_info.driver_car_idx as usize;

                            let subsessionid = session_info.weekend_info.sub_session_id;
                            let custid = session_info.driver_info.drivers[hero_car_idx].user_id;

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



                if let Some(data) = telemetry {
                    let mut telem_data = Vec::with_capacity(data.car_idx_lap_dist_pct.len());

                    for car_idx in 0..data.car_idx_lap_dist_pct.len() {
                        let lap_dist_pct = data.car_idx_lap_dist_pct[car_idx];

                        telem_data.push(irp::CarTelemetry {
                            lap_pct: lap_dist_pct,
                            fuel_level: if car_idx == hero_car_idx { data.fuel_level } else { 0.0 },
                            fuel_level_pct: if car_idx == hero_car_idx { data.fuel_level_pct } else { 0.0 },
                        });
                    }

                    yield irp::FooBarRequest {
                        msg: Some(foo_bar_request::Msg::Telemetry(irp::Telemetry { cars: telem_data }))
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

        println!("Connection closed, retrying...");
    }
}
