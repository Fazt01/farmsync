mod api;
mod room;

use crate::room::Room;
use futures_util::stream::TryStreamExt;
use futures_util::{SinkExt, StreamExt};
use http_body_util::{combinators::BoxBody, BodyExt, Empty, StreamBody};
use hyper::body::{Buf, Bytes, Frame, Incoming};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Method, Request, Response, StatusCode};
use hyper_tungstenite::{HyperWebsocket};
use hyper_util::rt::TokioIo;
use serde::{de, Deserialize};
use std::collections::HashMap;
use std::io::Read;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::fs::File;
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use tokio_util::io::ReaderStream;
use tokio::sync::mpsc;
use tokio::sync::mpsc::Sender;
use tungstenite::Message;

#[derive(Clone)]
struct Service {
    data: Arc<Data>,
}

impl Service {
    fn new() -> Self {
        Service {
            data: Arc::new(Data {
                room: Mutex::new(Room::new()),
                connections: Mutex::new(Default::default()),
                connection_counter: AtomicU64::new(0),
            }),
        }
    }

    async fn refresh_all_player_lists(&self) -> anyhow::Result<()> {
        let room = &*self.data.room.lock().await;
        let text = api::player_list(&room.all_players());
        self.send_to_all(Message::text(text)).await
    }

    async fn refresh_player_in_all_lists(&self, player_data: room::PlayerData) -> anyhow::Result<()> {
        self.send_to_all(Message::text(api::player(&player_data))).await
    }

    async fn send_to_all(&self, message: Message) -> anyhow::Result<()>{
        let websockets = &mut *self.data.connections.lock().await;
        for sender in websockets.values() {
            sender.send(message.clone()).await?;
        }
        Ok(())
    }
}

struct Data {
    room: Mutex<Room>,
    connections: Mutex<HashMap<u64, Sender<Message>>>,
    connection_counter: AtomicU64,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));

    // We create a TcpListener and bind it to 127.0.0.1:3000
    let listener = TcpListener::bind(addr).await?;

    let service = Service::new();

    // We start a loop to continuously accept incoming connections
    loop {
        let (stream, _) = listener.accept().await?;

        // Use an adapter to access something implementing `tokio::io` traits as if they implement
        // `hyper::rt` IO traits.
        let io = TokioIo::new(stream);

        let service = service.clone();

        // Spawn a tokio task to serve multiple connections concurrently
        tokio::task::spawn({
            async move {
                // Finally, we bind the incoming connection to our `hello` service
                if let Err(err) = http1::Builder::new()
                    // `service_fn` converts our function in a `Service`
                    .keep_alive(true)
                    .serve_connection(
                        io,
                        service_fn(move |req| {
                            let service = service.clone();
                            handle_request(req, service)
                        }),
                    )
                    .with_upgrades()
                    .await
                {
                    eprintln!("Error serving connection: {:?}", err);
                }
            }
        });
    }
}

async fn serve_file(path: &str) -> anyhow::Result<Response<BoxBody<Bytes, std::io::Error>>> {
    let file = File::open(path).await?;
    let stream = ReaderStream::new(file);
    let stream_body = StreamBody::new(stream.map_ok(Frame::data));
    let box_body = BoxBody::new(stream_body);
    Ok(Response::new(box_body))
}

async fn handle_request(
    mut req: Request<Incoming>,
    service: Service,
) -> anyhow::Result<Response<BoxBody<Bytes, anyhow::Error>>> {
    let service = service.clone();
    if hyper_tungstenite::is_upgrade_request(&req) {
        let (response, websocket) = hyper_tungstenite::upgrade(&mut req, None)?;

        // Spawn a task to handle the websocket connection.
        tokio::spawn(async move {
            if let Err(e) = handle_websocket(websocket, service).await {
                eprintln!("Error in websocket connection: {e}");
            }
        });

        println!("websocket connected");

        // Return the response so the spawned future can continue.
        Ok(response.map(|body| body.map_err(|never| match never {}).boxed()))
    } else {
        // Handle regular HTTP requests here.
        handle_http_request(req, service).await
    }
}

#[derive(Deserialize, Debug)]
struct ChangeNameReq {
    name: String,
    user_name: String,
}

#[derive(Deserialize, Debug)]
struct VisitedReq {
    visited_id: u64,
    user_name: String,
}

#[derive(Deserialize, Debug)]
struct ClearedReq {
    user_name: String,
}

async fn handle_http_request(
    req: Request<Incoming>,
    service: Service,
) -> anyhow::Result<Response<BoxBody<Bytes, anyhow::Error>>> {
    let path = req.uri().path().to_string();

    let result = match (req.method(), req.uri().path()) {
        (&Method::GET, "/") => serve_file("html/index.html").await,
        (&Method::POST, "/change-name") => {
            // Aggregate the body...

            let parsed_req: ChangeNameReq = req_to_parsed_form_data(req).await?;

            service
                .data
                .room
                .lock()
                .await
                .change_name(parsed_req.user_name.as_str(), parsed_req.name);
            service.refresh_all_player_lists().await?;
            Ok(Response::new(empty()))
        }
        (&Method::POST, "/visited") => {
            let parsed_req: VisitedReq = req_to_parsed_form_data(req).await?;
            let player_data = service.data.room.lock().await.visited(parsed_req.visited_id);
            if let Some(player_data) = player_data {
                service.refresh_player_in_all_lists(player_data).await?;
            }
            Ok(Response::new(empty()))
        }
        (&Method::POST, "/cleared") => {
            let parsed_req: ClearedReq = req_to_parsed_form_data(req).await?;
            let player_data = service.data.room.lock().await.cleared(parsed_req.user_name.as_str());
            if let Some(player_data) = player_data {
                service.refresh_player_in_all_lists(player_data).await?;
            }
            Ok(Response::new(empty()))
        }

        _ => {
            let mut not_found = Response::new(empty());
            *not_found.status_mut() = StatusCode::NOT_FOUND;
            Ok(not_found)
        }
    }?;

    println!("handled {}", path);

    Ok(result.map(|body| body.map_err(|err| err.into()).boxed()))
}

async fn handle_websocket(websocket: HyperWebsocket, service: Service) -> anyhow::Result<()> {
    let mut websocket = websocket.await?;

    let text = api::player_list(&service.data.room.lock().await.all_players());

    websocket.send(Message::text(text)).await?;

    let (tx, mut rx) = mpsc::channel(1);

    let connection_id = service.data.connection_counter.fetch_add(1, Ordering::Relaxed);

    service
        .data
        .connections
        .lock()
        .await
        .insert(connection_id, tx);

    loop {
        tokio::select! {
            Some(message) = rx.recv() => {
                websocket.send(message).await?;
            },
            incoming = websocket.next() => {
                if incoming.is_none() {
                    break
                }
            },
            else => unreachable!("websocket should have been closed in branch above")
        }
    }

    service.data.connections.lock().await.remove(&connection_id);

    Ok(())
}

fn empty() -> BoxBody<Bytes, std::io::Error> {
    Empty::<Bytes>::new()
        .map_err(|never| match never {})
        .boxed()
}

async fn req_to_parsed_form_data<T: de::DeserializeOwned>(req: Request<Incoming>) -> anyhow::Result<T> {
    let whole_body = req.collect().await?.aggregate();
    let mut buf = String::new();
    whole_body.reader().read_to_string(&mut buf)?;

    Ok(serde_qs::from_str(buf.as_str())?)
}