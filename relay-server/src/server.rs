use async_trait::async_trait;
use std::io::{Cursor, Error};
use std::net::SocketAddr;
use tokio::io::BufReader;
use tokio::net::{TcpStream, ToSocketAddrs};

use tokio::net::TcpListener;

use yarpc::{
    http::{Call, IoStream, MemIoStreamEx, Message, Method, QueryEx, Request, Response},
    to_io_result::ToIoResult,
};

use crate::{mem_state::MemState, state::State};

/// The server keeps a state (messages) and can accept and respond to messages using the
/// `update` function.
///
/// ## Example
///
/// ```
/// use relay_server::Server;
/// use yarpc::http::{Call, Method, Response, Request};
/// #[tokio::main]
/// async fn main() {
///     let mut server = Server::default();
///     // send a message "Hello!"
///     {
///         let request = Request::new(
///             Method::POST,
///             "/".to_string(),
///             Default::default(),
///             "Hello!".as_bytes().to_vec(),
///         );
///         let response = server.call(request).await.unwrap();
///         let expected = Response::new(
///             200,
///             "OK".to_string(),
///             Default::default(),
///             Default::default(),
///         );
///         assert_eq!(response, expected);
///     }
/// }
/// ```
#[derive(Default)]
pub struct Server(MemState);

impl Server {
    pub async fn run<T: ToSocketAddrs>(addr: T) {
        let listener = TcpListener::bind(addr).await.unwrap();
        async fn handle_stream(
            stream: std::io::Result<(TcpStream, SocketAddr)>,
            server: &mut Server,
        ) -> Result<(), Error> {
            let (stream, _) = stream?;
            server.update(&mut BufReader::new(stream)).await?;
            Ok(())
        }

        let mut server = Server::default();
        loop {
            let maybe_stream = listener.accept().await;
            println!("Received stream: {maybe_stream:?}");
            if let Err(e) = handle_stream(maybe_stream, &mut server).await {
                eprintln!("IO error: {e}");
            }
        }
    }

    pub async fn update(&mut self, io: &mut impl IoStream) -> Result<(), Error> {
        let request = Request::read(io).await?;

        let content = match request.method {
            Method::GET => {
                let query = *request.url.url_query().get("id").to_io_result()?;
                self.0.get(query.to_string()).await?
            }
            Method::POST => {
                self.0.post(request.content).await?;
                Vec::default()
            }
        };
        let response = Response::new(200, "OK".to_string(), Default::default(), content);
        response.write(io).await?;
        println!("SERVER RESPONSE: {response:?}");
        Ok(())
    }
    async fn raw_call(&mut self, msg: &[u8]) -> Result<Vec<u8>, Error> {
        let mut result = Vec::default();
        let mut stream = msg.mem_io_stream(&mut result);
        self.update(&mut stream).await?;

        Ok(result)
    }
}

#[async_trait]
impl Call for Server {
    async fn call(&mut self, request: Request) -> Result<Response, Error> {
        let response_buf = {
            let mut request_stream = Cursor::<Vec<u8>>::default();
            request.write(&mut request_stream).await?;
            self.raw_call(request_stream.get_ref()).await?
        };
        Response::read(&mut Cursor::new(response_buf)).await
    }
}

#[cfg(test)]
mod test {
    use std::str::from_utf8;

    use super::*;

    #[tokio::test(flavor = "multi_thread")]
    async fn test() {
        let mut server = Server::default();
        {
            const REQUEST: &str = "\
                POST / HTTP/1.0\r\n\
                Content-Length: 6\r\n\
                \r\n\
                Hello!";
            let response = server.raw_call(REQUEST.as_bytes()).await.unwrap();
            const RESPONSE: &str = "\
                HTTP/1.1 200 OK\r\n\
                \r\n";
            assert_eq!(from_utf8(&response).unwrap(), RESPONSE);
        }
        {
            const REQUEST: &str = "\
                GET /?id=x HTTP/1.0\r\n\
                \r\n";
            let response = server.raw_call(REQUEST.as_bytes()).await.unwrap();
            const RESPONSE: &str = "\
                HTTP/1.1 200 OK\r\n\
                content-length:6\r\n\
                \r\n\
                Hello!";
            assert_eq!(from_utf8(&response).unwrap(), RESPONSE);
        }
        {
            const REQUEST: &str = "\
                GET /?id=x HTTP/1.1\r\n\
                \r\n";
            let response = server.raw_call(REQUEST.as_bytes()).await.unwrap();
            const RESPONSE: &str = "\
                HTTP/1.1 200 OK\r\n\
                \r\n";
            assert_eq!(from_utf8(&response).unwrap(), RESPONSE);
        }
        /* invalid request
        Disabling this test because we should not be interested in supporting
        invalid HTTP clients that cannot write proper payloads. Our server only
        reads up to content_length bytes, so if the client sends more bytes than
        content_length, the server will not read beyond it

        {
            const REQUEST: &str = "\
                POST / HTTP/1.1\r\n\
                Content-Length: 6\r\n\
                \r\n\
                Hello!j";
            let response = server.raw_call(REQUEST.as_bytes()).await;
            assert!(response.is_err());
        }*/
    }
}
