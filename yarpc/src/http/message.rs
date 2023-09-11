use crate::to_io_result::{err, ToIoResult};
use async_trait::async_trait;
use std::collections::HashMap;
use std::io::Error;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncReadExt, AsyncWrite, AsyncWriteExt};

pub const PROTOCOL: &str = "HTTP/1.1";

const CONTENT_LENGTH: &str = "content-length";

pub trait AsyncReadBuf: AsyncBufRead + Unpin + Send {}
impl<T: AsyncBufRead + Unpin + Send> AsyncReadBuf for T {}

pub trait AsyncWriteBuf: AsyncWrite + Unpin + Send {}
impl<T: AsyncWrite + Unpin + Send> AsyncWriteBuf for T {}

#[async_trait]
pub trait Message: Sized {
    fn parse(
        first_line: Vec<String>,
        headers: HashMap<String, String>,
        content: Vec<u8>,
    ) -> Result<Self, Error>;
    fn first_line(&self) -> Vec<String>;
    fn headers(&self) -> &HashMap<String, String>;
    fn content(&self) -> &Vec<u8>;

    async fn read(i: &mut impl AsyncReadBuf) -> Result<Self, Error> {
        async fn read_line(i: &mut impl AsyncReadBuf) -> Result<String, Error> {
            let mut buf = Vec::new();
            let _len = i.read_until(b'\r', &mut buf).await?;
            if i.read_u8().await? != 10 {
                return err("invalid HTTP line");
            }

            String::from_utf8(buf)
                .map_err(|err| Error::new(std::io::ErrorKind::InvalidData, err.to_string()))
        }

        // read and parse the request line
        let first_line = read_line(i)
            .await?
            .split(' ')
            .map(|l| l.trim())
            .map(str::to_string)
            .collect();
        // read and parse headers
        let mut content_length = 0;
        let mut headers = HashMap::default();
        loop {
            let line = read_line(i).await?;

            let line = line.trim();

            if line.is_empty() {
                break;
            }

            let (name, value) = {
                let (name, value) = line.split_once(':').to_io_result()?;
                (name.to_lowercase(), value.trim())
            };

            if name == CONTENT_LENGTH {
                content_length = value.parse().to_io_result()?;
            } else {
                headers.insert(name, value.to_string());
            }
        }

        let mut buf = vec![0u8; content_length];

        // Read content_length bytes into buf
        let _len = AsyncReadExt::read_exact(i, &mut buf).await?;

        // return the message
        Self::parse(first_line, headers, buf)
    }
    async fn write(&self, o: &mut impl AsyncWriteBuf) -> Result<(), Error> {
        const EOL: &[u8] = "\r\n".as_bytes();
        const CONTENT_LENGTH_BYTES: &[u8] = CONTENT_LENGTH.as_bytes();
        const COLON: &[u8] = ":".as_bytes();

        o.write_all(self.first_line().join(" ").as_bytes()).await?;
        o.write_all(EOL).await?;
        // convert write_header to async function
        async fn write_header(o: &mut impl AsyncWriteBuf, k: &[u8], v: &[u8]) -> Result<(), Error> {
            o.write_all(k).await?;
            o.write_all(COLON).await?;
            o.write_all(v).await?;
            o.write_all(EOL).await?;
            Ok(())
        }

        for (k, v) in self.headers().iter() {
            write_header(o, k.as_bytes(), v.as_bytes()).await?;
        }
        let content = self.content();
        let len = content.len();
        if len > 0 {
            write_header(o, CONTENT_LENGTH_BYTES, len.to_string().as_bytes()).await?;
        }
        //These could cause partial writes. Should we check the returned number of written bytes?
        o.write_all(EOL).await?;
        o.write_all(content).await?;
        o.flush().await?;
        Ok(())
    }
}
