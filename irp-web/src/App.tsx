import {useEffect, useRef, useState} from "react";

class CarTelemetry {
    lap_pct = 0;
}

class HeroTelemetry {
    fuel_level = 0;
    fuel_level_pct = 0;
}

class Telemetry {
    cars: CarTelemetry[] = [];
    hero: HeroTelemetry = new HeroTelemetry();
    hero_car_idx: number = 0;
}

interface DaemonsMessage {
    type: "list_daemons";
    data: { daemons: number[] };
}

interface TelemetryMessage {
    type: "telemetry";
    data: { telemetry: Telemetry };
}

type WsMessage = DaemonsMessage | TelemetryMessage;

function parseWsMessage(raw: string): WsMessage {
    return JSON.parse(raw) as WsMessage;
}

function App() {
    const [daemons, setDaemons] = useState<number[]>([]);
    const [telemetry, setTelemetry] = useState<Telemetry | null>(null);

    const wsRef = useRef<WebSocket | null>(null);

    useEffect(() => {
        const ws = new WebSocket("/ws");
        wsRef.current = ws;
        ws.onmessage = (event) => {
            const msg = parseWsMessage(event.data);
            switch (msg.type) {
                case "list_daemons":
                    setDaemons(msg.data.daemons)
                    break;
                case "telemetry":
                    setTelemetry(msg.data.telemetry);
                    break;
            }
        }
        return () => ws.close();
    }, [])

    function selectDaemon(custid: number) {
        wsRef.current?.send(JSON.stringify({type: "select_daemon", data: custid}));
    }

    return (
        <div>
            {daemons.map((custid) => (
                <button key={custid} onClick={() => selectDaemon(custid)}>Daemon {custid}</button>
            ))}
            {telemetry ? (
                telemetry.cars.map((car, i) => {
                    return (i === telemetry.hero_car_idx) ? (
                        <div key={i}>Car {i}: Dist: {(car.lap_pct * 100).toFixed(1)} %
                            Fuel: {(telemetry.hero.fuel_level_pct * 100).toFixed(1)} %
                        </div>
                    ) : (
                        <div key={i}>Car {i}: {(car.lap_pct * 100).toFixed(1)} %</div>
                    );
                })
            ) : (
                <div>Select a daemon to start</div>
            )}
        </div>
    )
}

export default App
