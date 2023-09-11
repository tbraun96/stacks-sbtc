use async_trait::async_trait;
use std::{collections::HashMap, io::Error};

use crate::state::State;

/// The MemState struct holds the state of the relay-server in memory.
///
/// ## Example
///
/// ```
/// use relay_server::{MemState, State};
/// #[tokio::main]
/// async fn main() {
///     let mut mem_state = MemState::default();
///     mem_state.post(b"Hello world!".to_vec()).await.unwrap();;
///     let message = mem_state.get("node".to_string()).await.unwrap();
///     assert_eq!(message, b"Hello world!".to_vec());
/// }
/// ```
#[derive(Default)]
pub struct MemState {
    /// The value for this map is an index for the last read message for this node.
    highwaters: HashMap<String, usize>,
    queue: Vec<Vec<u8>>,
}

#[async_trait]
impl State for MemState {
    async fn get(&mut self, node_id: String) -> Result<Vec<u8>, Error> {
        let first_unread = self
            .highwaters
            .get(&node_id)
            .map_or(0, |last_read| *last_read + 1);
        let result = self.queue.get(first_unread);
        Ok(if let Some(r) = result {
            self.highwaters.insert(node_id, first_unread);
            r.clone()
        } else {
            Vec::default()
        })
    }
    async fn post(&mut self, msg: Vec<u8>) -> Result<(), Error> {
        self.queue.push(msg);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{MemState, State};
    #[tokio::test]
    async fn state_test() {
        let mut state = MemState::default();
        assert!(state.get(1.to_string()).await.unwrap().is_empty());
        assert!(state.get(3.to_string()).await.unwrap().is_empty());
        assert_eq!(0, state.highwaters.len());
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
