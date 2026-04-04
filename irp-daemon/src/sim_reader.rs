use std::{thread, time::Duration};

use irp_reader::{IrpReaderError, TelemetryQuery, TelemetrySource, WindowsMmapSource};

use crate::{LiveData, SessionState, State};

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

            let live_data_query = match TelemetryQuery::<LiveData>::new(&source) {
                Ok(query) => query,
                Err(IrpReaderError::UnknownVariable(_)) => {
                    println!("Waiting for iRacing...");
                    thread::sleep(Duration::from_secs(1));
                    continue;
                }
                Err(err) => panic!("Failed to create telemetry query: {}", err),
            };

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

                if !live_data.is_on_track {
                    continue;
                }

                {
                    let mut daemon_state = state.state.lock().expect("Poisoned state lock");
                    if let Some(session_info) = snapshot.session_info() {
                        println!("Session info received (Tick={})", snapshot.tick_count());
                        let session_info = serde_yaml::from_slice(session_info).unwrap();
                        daemon_state.session_info = SessionState::New(session_info);
                    } else if let Some(session_info) =
                        daemon_state.session_info.take_session_info()
                    {
                        daemon_state.session_info = SessionState::Existing(session_info);
                    };
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
