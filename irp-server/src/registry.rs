use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use tokio::sync::{broadcast, watch};

use crate::Telemetry;

struct DaemonEntry {
    sender: broadcast::Sender<Telemetry>,
}

pub struct DaemonGuard {
    custid: i32,
    sender: broadcast::Sender<Telemetry>,
    registry: DaemonRegistry,
}

impl DaemonGuard {
    pub fn send(
        &self,
        telemetry: Telemetry,
    ) -> Result<usize, broadcast::error::SendError<Telemetry>> {
        self.sender.send(telemetry)
    }
}

impl Drop for DaemonGuard {
    fn drop(&mut self) {
        self.registry.unregister(self.custid);
    }
}

#[derive(Clone)]
pub struct DaemonRegistry {
    daemons: Arc<Mutex<HashMap<i32, DaemonEntry>>>,
    watch: watch::Sender<Vec<i32>>,
}

impl DaemonRegistry {
    pub fn new() -> Self {
        Self {
            daemons: Arc::new(Mutex::new(HashMap::new())),
            watch: watch::Sender::new(Vec::new()),
        }
    }

    pub fn register(&self, custid: i32) -> DaemonGuard {
        let sender = broadcast::Sender::new(10);
        let entry = DaemonEntry {
            sender: sender.clone(),
        };

        {
            let mut daemons = self.daemons.lock().expect("Poisoned registry lock");
            daemons.insert(custid, entry);
            self.watch.send_replace(daemons.keys().copied().collect());
        }

        DaemonGuard {
            custid,
            sender,
            registry: self.clone(),
        }
    }

    fn unregister(&self, custid: i32) {
        let mut daemons = self.daemons.lock().expect("Poisoned registry lock");
        daemons.remove(&custid);
        self.watch.send_replace(daemons.keys().copied().collect());
    }

    pub fn receiver(&self, custid: i32) -> Option<broadcast::Receiver<Telemetry>> {
        self.daemons
            .lock()
            .expect("Poisoned registry lock")
            .get(&custid)
            .map(|entry| entry.sender.subscribe())
    }

    pub fn subscribe(&self) -> watch::Receiver<Vec<i32>> {
        self.watch.subscribe()
    }
}
