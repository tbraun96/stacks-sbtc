use std::io;
use std::process::Stdio;

use async_trait::async_trait;
use tokio::process::{Child, ChildStdin, ChildStdout, Command};

use serde::{de::DeserializeOwned, Serialize};
use serde_json::{from_str, to_string};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use crate::{
    rpc::Rpc,
    to_io_result::{TakeToIoResult, ToIoResult},
};

pub struct Js {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

impl Drop for Js {
    fn drop(&mut self) {
        let _ = self.child.start_kill();
    }
}

impl Js {
    /// Note: the function spawns a `deno` process.
    pub fn new(path: &str) -> io::Result<Js> {
        let mut child = Command::new("deno")
            .arg("run")
            .arg("--allow-env")
            .arg("--allow-read")
            .arg("--allow-net")
            .arg(path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()?;
        let stdin = child.stdin.take_to_io_result()?;
        let stdout = child.stdout.take_to_io_result()?;
        Ok(Js {
            child,
            stdin,
            stdout: BufReader::new(stdout),
        })
    }
}

type JsResult<T> = Result<T, String>;

#[async_trait]
impl Rpc for Js {
    async fn call<I: Serialize + Sync, O: Serialize + DeserializeOwned>(
        &mut self,
        input: &I,
    ) -> io::Result<O> {
        {
            let stdin = &mut self.stdin;
            let i = to_string(input)?;
            stdin.write_all(i.as_bytes()).await?;
            stdin.write_all("\n".as_bytes()).await?;
            stdin.flush().await?;
        }
        {
            let mut o = Vec::default();
            let _len = self.stdout.read_until(b'\n', &mut o).await?;
            let o = String::from_utf8(o).map_err(|err| {
                std::io::Error::new(std::io::ErrorKind::InvalidData, err.to_string())
            })?;
            let result: JsResult<O> = from_str(o.trim_end())?;
            result.to_io_result()
        }
    }
}
