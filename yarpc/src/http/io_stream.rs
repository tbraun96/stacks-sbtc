use async_trait::async_trait;
use std::io::Error;

use crate::http::message::{AsyncReadBuf, AsyncWriteBuf};
use crate::http::{Call, Message, Request, Response};

/// A trait for bidirectional stream.
///
/// For example, `TcpStream` is a bidirectional stream.
pub trait IoStream: AsyncReadBuf + AsyncWriteBuf + Unpin + Send + Sync {}
impl<T: AsyncReadBuf + AsyncWriteBuf + Unpin + Send + Sync> IoStream for T {}
#[async_trait]
impl<T: IoStream> Call for T {
    async fn call(&mut self, request: Request) -> Result<Response, Error> {
        // send data to a callee.
        request.write(self).await?;
        // read data from the callee.
        Response::read(self).await
    }
}
