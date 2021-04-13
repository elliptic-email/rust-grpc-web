mod proto {
    tonic::include_proto!("quotes");
}

use futures::StreamExt;
use std::pin::Pin;
use tokio::sync::broadcast::{channel, Sender};
use tokio::time::{sleep, Duration};
use tokio_stream::{wrappers::BroadcastStream, Stream};
use tonic::{async_trait, Request, Response, Status};

// Implementation of the server trait generated by tonic
#[derive(Debug, Clone)]
struct ServerImpl {
    tx: Sender<proto::SubscribeReply>,
}

#[async_trait]
impl proto::quote_service_server::QuoteService for ServerImpl {
    async fn get_currencies(
        &self,
        _request: Request<proto::CurrenciesRequest>,
    ) -> Result<Response<proto::CurrencyReply>, Status> {
        let reply = proto::CurrencyReply {
            iso_codes: vec!["BTC".into(), "ETH".into()],
        };

        Ok(Response::new(reply))
    }

    type SubscribeStream =
        Pin<Box<dyn Stream<Item = Result<proto::SubscribeReply, Status>> + Send + Sync + 'static>>;

    async fn subscribe(
        &self,
        request: Request<proto::SubscribeRequest>,
    ) -> Result<Response<Self::SubscribeStream>, Status> {
        let proto::SubscribeRequest {} = request.into_inner();

        let rx = self.tx.subscribe();
        let stream = BroadcastStream::new(rx)
            .filter_map(|item| async move {
                // ignore receive errors
                item.ok()
            })
            .map(Ok);
        let stream: Self::SubscribeStream = Box::pin(stream);
        let res = Response::new(stream);

        Ok(res)
    }
}

#[tokio::main]
async fn main() {
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));

    let (tx, _rx) = channel(1024);
    let addr = "0.0.0.0:50051".parse().unwrap();

    let server = ServerImpl { tx: tx.clone() };

    tokio::spawn(async move {
        loop {
            sleep(Duration::from_millis(5000)).await;
            let client = reqwest::Client::new();
            let resp = client
                .get("https://api.gdax.com/products/BTC-USD/book")
                .header(reqwest::header::USER_AGENT, "My Rust Program 1.0")
                .send()
                .await
                .expect("Problem with url")
                .text()
                .await
                .expect("Problem with parsing json");

            let price: Vec<&str> = resp.split("\"").collect();
            let price = price[3].into();

            dbg!(&price);

            tx.send(proto::SubscribeReply { key: price })
                .expect("failed to send");
        }
    });

    // Build our tonic `Service`
    let service = proto::quote_service_server::QuoteServiceServer::new(server);

    tonic::transport::Server::builder()
        .add_service(service)
        .serve(addr)
        .await
        .unwrap();
}
