use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::{fmt::Debug, time::Duration};
use tokio::sync::Mutex;
use tracing::{debug, warn};

use crate::signing_round;
// Message is the format over the wire
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Message {
    pub msg: signing_round::MessageTypes,
    pub sig: Vec<u8>,
}

// Http listen/poll with queue (requires mutable access, is configured by passing in HttpNet)
pub struct HttpNetListen {
    pub net: Arc<Mutex<HttpNet>>,
    in_queue: Mutex<Vec<Message>>,
}

impl Clone for HttpNetListen {
    fn clone(&self) -> Self {
        Self {
            net: self.net.clone(),
            // Clone is only used for testing, so we can just create a new empty queue for each
            // new instance
            in_queue: Mutex::new(Vec::new()),
        }
    }
}

impl HttpNetListen {
    pub fn new(net: HttpNet, in_queue: Vec<Message>) -> Self {
        HttpNetListen {
            net: Arc::new(Mutex::new(net)),
            in_queue: Mutex::new(in_queue),
        }
    }
}

// Http send (does not require mutable access, can be cloned to pass to threads)
#[derive(Clone)]
pub struct HttpNet {
    pub http_relay_url: String,
    connected: bool,
}

impl HttpNet {
    pub fn new(http_relay_url: String) -> Self {
        HttpNet {
            http_relay_url,
            connected: true,
        }
    }
}

// these functions manipulate the inbound message queue
#[async_trait]
pub trait NetListen {
    type Error: Debug;
    type Arg: Clone;

    async fn poll(&self, arg: Self::Arg);
    async fn next_message(&self) -> Option<Message>;
    async fn send_message(&self, msg: Message) -> Result<(), Self::Error>;
}

#[async_trait]
impl NetListen for HttpNetListen {
    type Error = Error;
    type Arg = u32;

    async fn poll(&self, id: u32) {
        let url = url_with_id(&self.net.lock().await.http_relay_url, id);
        debug!("poll {}", url);
        match reqwest::get(&url).await {
            Ok(response) => {
                self.net.lock().await.connected = true;
                if response.status() == 200 {
                    if let Ok(msg) = response.bytes().await {
                        match bincode::deserialize_from::<_, Message>(msg.as_ref()) {
                            Ok(msg) => {
                                debug!("received {:?}", msg);
                                self.in_queue.lock().await.push(msg);
                            }
                            Err(_e) => {}
                        };
                    }
                };
            }
            Err(e) => {
                let mut lock = self.net.lock().await;
                if lock.connected {
                    warn!("{} U: {}", e, url);
                    lock.connected = false;
                }
            }
        }
    }
    async fn next_message(&self) -> Option<Message> {
        self.in_queue.lock().await.pop()
    }

    // pass-thru to immutable net function
    async fn send_message(&self, msg: Message) -> Result<(), Self::Error> {
        self.net.lock().await.send_message(msg).await
    }
}

// for threads that only send data, use immutable Net
#[async_trait]
pub trait Net {
    type Error: Debug;

    async fn send_message(&self, msg: Message) -> Result<(), Self::Error>;
}

#[async_trait]
impl Net for HttpNet {
    type Error = Error;

    async fn send_message(&self, msg: Message) -> Result<(), Self::Error> {
        // sign message
        let bytes = &bincode::serialize(&msg)?;

        let notify = |_err, dur| {
            debug!(
                "Failed to connect to {}. Next attempt in {:?}",
                &self.http_relay_url, dur
            );
        };

        let send_request = || async move {
            println!("Attempting to send request");
            reqwest::Client::default()
                .post(&self.http_relay_url)
                .body(bytes.clone())
                .header("Content-Length", bytes.len())
                .send()
                .await
                .map_err(backoff::Error::transient)
        };
        let backoff_timer = backoff::ExponentialBackoffBuilder::new()
            .with_initial_interval(Duration::from_millis(2))
            .with_max_interval(Duration::from_millis(128))
            .build();

        let response = backoff::future::retry_notify(backoff_timer, send_request, notify)
            .await
            .map_err(|_| Error::Timeout)?;

        debug!(
            "sent {:?} {} bytes {:?} to {}",
            &msg.msg,
            bytes.len(),
            &response,
            self.http_relay_url
        );
        Ok(())
    }
}

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Serialization failed: {0}")]
    SerializationError(#[from] bincode::Error),
    #[error("{0}")]
    NetworkError(#[from] Box<reqwest::Error>),
    #[error("Failed to connect to network.")]
    Timeout,
}

fn url_with_id(base: &str, id: u32) -> String {
    let mut url = base.to_owned();
    url.push_str(&format!("?id={id}"));
    url
}
