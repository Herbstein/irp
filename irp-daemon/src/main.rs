use std::{thread, time::Duration};

use futures_util::stream::StreamExt;
use irp_proto::irp::{
    CarTelemetry, FooBarRequest, Handshake, Telemetry, foo_bar_request, irp_client::IrpClient,
};
use irp_reader::{
    FromTelemetrySnapshot, IrpReaderError, SnapshotReader, TelemetryQuery, TelemetrySource,
    WindowsMmapSource,
};

#[derive(Clone, Debug)]
struct LiveData {
    is_on_track: bool,
    car_idx_lap_dist_pct: Vec<f32>,
}

impl FromTelemetrySnapshot for LiveData {
    const REQUIRED_VARS: &[&str] = &["CarIdxLapDistPct", "IsOnTrack"];

    fn from_snapshot(reader: &SnapshotReader) -> Result<Self, IrpReaderError> {
        Ok(LiveData {
            is_on_track: reader.get_bool("IsOnTrack")?,
            car_idx_lap_dist_pct: reader.get_float_array("CarIdxLapDistPct")?,
        })
    }
}

#[derive(Clone, Debug)]
enum IrpMessage {
    Telemetry(LiveData),
    LapSummary,
    SessionInfo,
}

#[tokio::main]
async fn main() {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

    thread::spawn(move || {
        loop {
            let mut source = match WindowsMmapSource::connect() {
                Ok(source) => source,
                Err(IrpReaderError::MemoryMapMissing) | Err(IrpReaderError::UnknownVariable(_)) => {
                    println!("Waiting for iRacing...");
                    thread::sleep(Duration::from_secs(1));
                    continue;
                }
                Err(err) => panic!("Failed to connect to iRacing: {}", err),
            };

            println!("Sim connected");

            let live_data_query = TelemetryQuery::<LiveData>::new(&source).unwrap();

            let mut consecutive_empty = 0;

            loop {
                let snapshot = match source.wait_for_snapshot() {
                    Ok(Some(snapshot)) => snapshot,
                    Ok(None) => {
                        consecutive_empty += 1;
                        if consecutive_empty > 10 {
                            break;
                        }
                        continue;
                    }
                    Err(err) => panic!("Failed while waiting for snapshot: {}", err),
                };

                consecutive_empty = 0;

                let live_data = live_data_query.deserialize(&snapshot).unwrap();

                if snapshot.session_info().is_some() {
                    println!("Session info received (Tick={})", snapshot.tick_count());
                    tx.send(IrpMessage::SessionInfo).unwrap();
                };

                tx.send(IrpMessage::Telemetry(live_data)).unwrap();
            }

            println!("Sim disconnected")
        }
    });

    let outbound = async_stream::stream! {
        let mut has_sent_handshake = false;

        while let Some(message) = rx.recv().await {
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

    let mut client = IrpClient::connect("http://localhost:50051").await.unwrap();

    let response = client.foo_bar(outbound).await.unwrap();
    let mut inbound = response.into_inner();

    while inbound.next().await.is_some() {}
}
