use std::{thread, time::Duration};

use irp_reader::{IrpReaderError, TelemetryQuery, TelemetrySource, WindowsMmapSource};
use tokio::sync::mpsc;

use crate::{IrpMessage, LiveData};

pub fn sim_reader(sender: mpsc::UnboundedSender<IrpMessage>) {
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
                    sender.send(IrpMessage::SessionInfo).unwrap();
                };

                sender.send(IrpMessage::Telemetry(live_data)).unwrap();
            }

            println!("Sim disconnected")
        }
    });
}
