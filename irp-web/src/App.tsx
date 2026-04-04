import {useEffect, useState} from "react";

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

function App() {
    const [telemetry, setTelemetry] = useState<Telemetry>(new Telemetry());

    useEffect(() => {
        const ws = new WebSocket("/ws");
        ws.onmessage = (event) => {
            const data = JSON.parse(event.data);
            setTelemetry({
                cars: data.cars,
                hero: data.hero,
                hero_car_idx: data.hero_car_idx,
            });
        }

        return () => ws.close();
    }, [])

    return (
        <div>
            {telemetry.cars.map((car, i) => {
                return (i === telemetry.hero_car_idx) ? (
                    <div key={i}>Car {i}: Dist: {(car.lap_pct * 100).toFixed(1)} %
                        Fuel: {(telemetry.hero.fuel_level_pct * 100).toFixed(1)} %
                    </div>
                ) : (
                    <div key={i}>Car {i}: {(car.lap_pct * 100).toFixed(1)} %</div>
                );
            })}
        </div>
    )
}

export default App
