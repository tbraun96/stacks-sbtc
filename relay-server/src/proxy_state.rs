use async_trait::async_trait;
use std::io::Error;

use yarpc::http::{Call, Method, Request};

use crate::state::State;

pub struct ProxyState<T: Call>(pub T);

#[async_trait]
impl<T: Call> State for ProxyState<T> {
    async fn get(&mut self, node_id: String) -> Result<Vec<u8>, Error> {
        Ok(self
            .0
            .call(Request::new(
                Method::GET,
                format!("/?id={node_id}"),
                Default::default(),
                Default::default(),
            ))
            .await?
            .content)
    }

    async fn post(&mut self, msg: Vec<u8>) -> Result<(), Error> {
        self.0
            .call(Request::new(
                Method::POST,
                "/".to_string(),
                Default::default(),
                msg,
            ))
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::super::*;
    use super::*;

    #[tokio::test]
    async fn test() {
        let mut state = ProxyState(Server::default());
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
    }
}
