use frost_signer::net::{HttpNet, HttpNetListen, Message, NetListen};
use frost_signer::signing_round::{DkgBegin, MessageTypes};

#[tokio::test]
async fn receive_msg() {
    let m1 = Message {
        msg: MessageTypes::DkgBegin(DkgBegin { dkg_id: 0 }),
        sig: vec![0u8; 64],
    };

    let stacks_node_url = "http://localhost:9775".to_owned();

    let in_queue = vec![m1];
    let net = HttpNet::new(stacks_node_url);
    let net_listen = HttpNetListen::new(net, in_queue);
    match net_listen.next_message().await {
        Some(_msg) => {
            assert!(true)
        }
        None => {}
    }
}
