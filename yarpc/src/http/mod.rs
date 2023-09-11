mod io_stream;
mod mem_io_stream;
mod message;
mod method;
mod request;
mod response;
mod url;

use async_trait::async_trait;
use std::io::Error;

pub use io_stream::IoStream;
pub use mem_io_stream::{MemIoStream, MemIoStreamEx};
pub use message::Message;
pub use method::Method;
pub use request::Request;
pub use response::Response;
pub use url::QueryEx;

#[async_trait]
pub trait Call: Send {
    async fn call(&mut self, request: Request) -> Result<Response, Error>;
}
