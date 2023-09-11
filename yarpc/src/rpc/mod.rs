use async_trait::async_trait;
use std::io;

use serde::{de::DeserializeOwned, Serialize};

pub mod dispatch_command;
pub mod js;

/// RPC (Remote Procedure Call)
#[async_trait]
pub trait Rpc {
    async fn call<I: Serialize + Sync, O: Serialize + DeserializeOwned>(
        &mut self,
        input: &I,
    ) -> io::Result<O>;
}
