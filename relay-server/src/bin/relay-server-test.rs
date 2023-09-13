use async_trait::async_trait;
use std::io::Error;
use std::time::Duration;
use tokio::io::BufReader;

use relay_server::{ProxyState, State};
use tokio::net::TcpStream;
use yarpc::http::{Call, Request, Response};
const ADDR: &str = "127.0.0.1:9776";

struct RemoteServer();

#[async_trait]
impl Call for RemoteServer {
    async fn call(&mut self, request: Request) -> Result<Response, Error> {
        // note: `relay-server` doesn't support multiple requests in one connection
        // so we create a new connection every time when we send a request.
        BufReader::new(TcpStream::connect(ADDR).await.unwrap())
            .call(request)
            .await
    }
}

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    // waiting for a server
    while TcpStream::connect(ADDR).await.is_err() {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    //
    let mut state = ProxyState(RemoteServer());
    //
    assert!(state.get(1.to_string()).await.unwrap().is_empty());
    assert!(state.get(3.to_string()).await.unwrap().is_empty());
    state.post("Msg # 0".as_bytes().to_vec()).await.unwrap();
    assert_eq!(
        "Msg # 0".as_bytes().to_vec(),
        state.get(1.to_string()).await.unwrap()
    );
    assert_eq!(
        "Msg # 0".as_bytes().to_vec(),
        state.get(5.to_string()).await.unwrap()
    );
    assert_eq!(
        "Msg # 0".as_bytes().to_vec(),
        state.get(4.to_string()).await.unwrap()
    );
    assert!(state.get(1.to_string()).await.unwrap().is_empty());
    state.post("Msg # 1".as_bytes().to_vec()).await.unwrap();
    assert_eq!(
        "Msg # 1".as_bytes().to_vec(),
        state.get(1.to_string()).await.unwrap()
    );
    assert_eq!(
        "Msg # 0".as_bytes().to_vec(),
        state.get(3.to_string()).await.unwrap()
    );
    assert_eq!(
        "Msg # 1".as_bytes().to_vec(),
        state.get(5.to_string()).await.unwrap()
    );
    state.post("Msg # 2".as_bytes().to_vec()).await.unwrap();
    assert_eq!(
        "Msg # 2".as_bytes().to_vec(),
        state.get(1.to_string()).await.unwrap()
    );
    assert_eq!(
        "Msg # 1".as_bytes().to_vec(),
        state.get(4.to_string()).await.unwrap()
    );
    assert_eq!(
        "Msg # 2".as_bytes().to_vec(),
        state.get(4.to_string()).await.unwrap()
    );
    println!("passed");
}
