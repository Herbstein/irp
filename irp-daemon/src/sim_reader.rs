use std::{thread, time::Duration};

use irp_reader::{IrpReaderError, TelemetryQuery, TelemetrySource, WindowsMmapSource};

use crate::{LiveData, State};

pub fn sim_reader(state: State) {
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

            state
                .state
                .lock()
                .expect("State lock poisoned")
                .sim_connected = true;

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

                {
                    let mut daemon_state = state.state.lock().expect("Poisoned state lock");
                    if let Some(session_info) = snapshot.session_info() {
                        println!("Session info received (Tick={})", snapshot.tick_count());
                        let session_info = serde_yaml::from_slice(session_info).unwrap();
                        daemon_state.latest_session_info = Some(session_info);
                    };
                    // TODO(herbstein): Implement lap completion logic
                    // daemon_state.pending_lap_summaries.push(LapSummary {});
                    daemon_state.latest_telemetry = Some(live_data);
                }

                // Store a permit in the Notify, making the next call return immediately
                state.notify.notify_one();
            }

            println!("Sim disconnected");

            state
                .state
                .lock()
                .expect("State lock disconnected")
                .sim_connected = false;
        }
    });
}
