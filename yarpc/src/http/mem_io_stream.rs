use std::io::Error;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::{AsyncBufRead, AsyncRead, AsyncWrite, BufReader, BufWriter, ReadBuf};

pub struct MemIoStream<'a> {
    pub i: BufReader<&'a [u8]>,
    pub o: BufWriter<&'a mut Vec<u8>>,
}

pub trait MemIoStreamEx<'a> {
    fn mem_io_stream(self, output: &'a mut Vec<u8>) -> MemIoStream<'a>;
}

impl<'a> MemIoStreamEx<'a> for &'a [u8] {
    fn mem_io_stream(self, output: &'a mut Vec<u8>) -> MemIoStream<'a> {
        MemIoStream {
            i: BufReader::new(self),
            o: BufWriter::new(output),
        }
    }
}

impl AsyncRead for MemIoStream<'_> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.i).poll_read(cx, buf)
    }
}

impl AsyncWrite for MemIoStream<'_> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, Error>> {
        Pin::new(&mut self.o).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Error>> {
        Pin::new(&mut self.o).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Error>> {
        Pin::new(&mut self.o).poll_shutdown(cx)
    }
}

impl AsyncBufRead for MemIoStream<'_> {
    fn poll_fill_buf(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<&[u8]>> {
        Pin::new(&mut self.get_mut().i).poll_fill_buf(cx)
    }

    fn consume(mut self: Pin<&mut Self>, amt: usize) {
        Pin::new(&mut self.i).consume(amt)
    }
}
