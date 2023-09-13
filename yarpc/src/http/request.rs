use std::{collections::HashMap, io::Error};

use crate::to_io_result::ToIoResult;

use super::{message::PROTOCOL, method::Method, Message};

#[derive(Debug, PartialEq, Eq)]
pub struct Request {
    pub method: Method,
    pub url: String,
    pub protocol: String,
    pub headers: HashMap<String, String>,
    pub content: Vec<u8>,
}

impl Request {
    pub fn new(
        method: Method,
        url: String,
        headers: HashMap<String, String>,
        content: Vec<u8>,
    ) -> Self {
        Self {
            method,
            url,
            protocol: PROTOCOL.to_owned(),
            headers,
            content,
        }
    }
}

impl Message for Request {
    fn parse(
        first_line: Vec<String>,
        headers: HashMap<String, String>,
        content: Vec<u8>,
    ) -> Result<Self, Error> {
        let mut i = first_line.into_iter();
        let mut next = || i.next().to_io_result();
        let method = next()?.parse()?;
        let url = next()?;
        let protocol = next()?;
        Ok(Request {
            method,
            url,
            protocol,
            headers,
            content,
        })
    }

    fn first_line(&self) -> Vec<String> {
        [
            self.method.to_string(),
            self.url.to_owned(),
            self.protocol.to_owned(),
        ]
        .to_vec()
    }

    fn headers(&self) -> &HashMap<String, String> {
        &self.headers
    }

    fn content(&self) -> &Vec<u8> {
        &self.content
    }
}

#[cfg(test)]
mod tests {
    use std::{io::Cursor, str::from_utf8};

    use crate::http::request::Method::{GET, POST};

    use super::{Message, Request};

    #[tokio::test(flavor = "multi_thread")]
    async fn test() {
        const REQUEST: &str = "\
            POST / HTTP/1.1\r\n\
            Content-Length: 6\r\n\
            \r\n\
            Hello!";
        let mut read = Cursor::new(REQUEST);
        let rm = Request::read(&mut read).await.unwrap();
        assert_eq!(rm.method, POST);
        assert_eq!(rm.url, "/");
        assert_eq!(rm.protocol, "HTTP/1.1");
        assert_eq!(rm.headers.len(), 0);
        assert_eq!(from_utf8(&rm.content), Ok("Hello!"));
        assert_eq!(read.position(), REQUEST.len() as u64);
        let mut v = Vec::default();
        rm.write(&mut Cursor::new(&mut v)).await.unwrap();
        const EXPECTED: &str = "\
            POST / HTTP/1.1\r\n\
            content-length:6\r\n\
            \r\n\
            Hello!";
        assert_eq!(from_utf8(&v), Ok(EXPECTED));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_header() {
        const REQUEST: &str = "\
            POST / HTTP/1.1\r\n\
            Content-Length: 6\r\n\
            Hello: someThing\r\n\
            \r\n\
            Hello!";
        let mut read = Cursor::new(REQUEST);
        let rm = Request::read(&mut read).await.unwrap();
        assert_eq!(rm.method, POST);
        assert_eq!(rm.url, "/");
        assert_eq!(rm.protocol, "HTTP/1.1");
        assert_eq!(rm.headers.len(), 1);
        assert_eq!(rm.headers["hello"], "someThing");
        assert_eq!(from_utf8(&rm.content), Ok("Hello!"));
        assert_eq!(read.position(), REQUEST.len() as u64);
        let mut v = Vec::default();
        rm.write(&mut Cursor::new(&mut v)).await.unwrap();
        const EXPECTED: &str = "\
            POST / HTTP/1.1\r\n\
            hello:someThing\r\n\
            content-length:6\r\n\
            \r\n\
            Hello!";
        assert_eq!(from_utf8(&v), Ok(EXPECTED));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn incomplete_message_test() {
        const REQUEST: &str = "\
            POST / HTTP/1.1\r\n\
            Content-Leng";
        let mut read = Cursor::new(REQUEST);
        assert!(Request::read(&mut read).await.is_err());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn incomplete_content_test() {
        const REQUEST: &str = "\
            POST / HTTP/1.1\r\n\
            Content-Length: 6\r\n\
            \r\n";
        let mut read = Cursor::new(REQUEST);
        assert!(Request::read(&mut read).await.is_err());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn invalid_message_test() {
        const REQUEST: &str = "\
            POST / HTTP/1.1\r\n\
            Content-Length 6\r\n\
            \r\n\
            Hello!";
        let mut read = Cursor::new(REQUEST);
        assert!(Request::read(&mut read).await.is_err());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn no_content_test() {
        const REQUEST: &str = "\
            GET /images/logo.png HTTP/1.1\r\n\
            \r\n";
        let mut read = Cursor::new(REQUEST);
        let rm = Request::read(&mut read).await.unwrap();
        assert_eq!(rm.method, GET);
        assert_eq!(rm.url, "/images/logo.png");
        assert_eq!(rm.protocol, "HTTP/1.1");
        assert!(rm.headers.is_empty());
        assert!(rm.content.is_empty());
        assert_eq!(read.position(), REQUEST.len() as u64);
        let mut v = Vec::default();
        rm.write(&mut Cursor::new(&mut v)).await.unwrap();
        const EXPECTED: &str = "\
            GET /images/logo.png HTTP/1.1\r\n\
            \r\n";
        assert_eq!(from_utf8(&v), Ok(EXPECTED));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn invalid_utf8_should_not_panic() {
        const REQUEST: &[u8] = &[0xFF];
        let mut read = Cursor::new(REQUEST);
        let rm = Request::read(&mut read).await;
        assert!(rm.is_err());
    }
}
