use std::pin::Pin;

use async_stream::try_stream;
use axum::Router;
use futures_util::{Stream, StreamExt};
use irp_proto::irp::{
    FooBarRequest, FooBarResponse, HandshakeAck, foo_bar_request, foo_bar_response,
    irp_server::{Irp, IrpServer},
};
use tokio::net::TcpListener;
use tonic::{async_trait, transport::Server};
use tower_http::services::ServeDir;

#[derive(Debug)]
struct IrpService;

#[async_trait]
impl Irp for IrpService {
    type FooBarStream =
        Pin<Box<dyn Stream<Item = Result<FooBarResponse, tonic::Status>> + Send + Sync>>;

    async fn foo_bar(
        &self,
        request: tonic::Request<tonic::codec::Streaming<FooBarRequest>>,
    ) -> Result<tonic::Response<Self::FooBarStream>, tonic::Status> {
        let mut stream = request.into_inner();

        let output = try_stream! {
            while let Some(request) = stream.next().await {
                let request = request?;
                match request.msg {
                    Some(foo_bar_request::Msg::Handshake(handshake)) => {
                        println!("Handshake received: custid={}, subsessionid={}", handshake.custid, handshake.subsessionid);
                        let response = FooBarResponse {
                            msg: Some(foo_bar_response::Msg::HandshakeAck(HandshakeAck {}))
                        };
                        yield response;
                    }
                    Some(foo_bar_request::Msg::Telemetry(telemetry)) => {
                        println!("Telemetry received: {}", telemetry.cars.len());
                    }
                    Some(foo_bar_request::Msg::Summary(summary)) => {
                        println!("Summary received: {}", summary.summaries.len());
                    }
                    _ => continue,
                }

            }
        };

        Ok(tonic::Response::new(Box::pin(output) as Self::FooBarStream))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let grpc_addr = "127.0.0.1:50051".parse()?;

    let irp = IrpService;

    let service = IrpServer::new(irp);

    let grpc = tokio::spawn(async move {
        Server::builder()
            .add_service(service)
            .serve(grpc_addr)
            .await
    });

    let http = tokio::spawn(async move {
        let app = Router::new().fallback_service(ServeDir::new("."));
        let listener = TcpListener::bind("127.0.0.1:8080").await?;
        axum::serve(listener, app).await
    });

    let (grpc_result, http_result) = tokio::try_join!(grpc, http)?;

    grpc_result?;
    http_result?;

    Ok(())
}
