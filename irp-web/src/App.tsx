import {useEffect, useState} from "react";

class CarTelemetry {
    lap_pct = 0;
}

function App() {
    const [cars, setCars] = useState<CarTelemetry[]>([]);

    useEffect(() => {
        const ws = new WebSocket("/ws");
        ws.onmessage = (event) => {
            const data = JSON.parse(event.data);
            setCars(data.cars);
        }

        return () => ws.close();
    }, [])

    return (
        <div>
            {cars.map((car, i) => {
                return <div key={i}>Car {i}: {(car.lap_pct * 100).toFixed(1)}%</div>
            })}
        </div>
    )
}

export default App
