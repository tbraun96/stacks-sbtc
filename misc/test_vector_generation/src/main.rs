use bitcoin::blockdata::opcodes;
use bitcoin::blockdata::script::Builder;
use bitcoin::consensus::encode::serialize;
use bitcoin::network::constants::Network;
use bitcoin::{OutPoint, Script, Transaction, TxIn, TxOut};

use secp256k1::SecretKey;

fn main() {
    generate_and_print_peg_out_request_test_vector();
}

fn generate_and_print_peg_out_request_test_vector() {
    // Arbitrary key, copy-pasted from src/chainstate/stacks/tests/accounting.rs
    let secret_key_hex = "42faca653724860da7a41bfcef7e6ba78db55146f6900de8cb2a9f760ffac70c01";
    let secret_key_vec = array_bytes::hex2bytes(secret_key_hex).unwrap();
    let secret_key = SecretKey::from_slice(&secret_key_vec[..32]).unwrap();

    let mut tx = Transaction {
        version: 2,
        lock_time: 0,
        input: vec![],
        output: vec![],
    };

    let input = TxIn {
        previous_output: OutPoint::null(),
        script_sig: Script::new(),
        sequence: 0xFFFFFFFF,
        witness: vec![],
    };
    tx.input.push(input);

    let amount = 1337u64;
    let fulfillment_fee = 42;
    let peg_wallet_address = [0; 32];

    let p2tr_script = Builder::new()
        .push_int(1)
        .push_slice(&peg_wallet_address)
        .into_script();

    let mut msg = amount.to_be_bytes().to_vec();
    msg.extend_from_slice(&p2tr_script.as_bytes());

    let msg_hash = sha256::digest(msg.as_slice());
    let msg_hash_bytes = array_bytes::hex2bytes(msg_hash).unwrap();
    let msg_ecdsa = secp256k1::Message::from_slice(&msg_hash_bytes).unwrap();

    let secp256k1_context = secp256k1::Secp256k1::new(); // Impure function call
    let signature = secp256k1_context.sign_ecdsa_recoverable(&msg_ecdsa, &secret_key);
    let (recovery_id, standard_signature_bytes) = signature.serialize_compact();
    let mut signature_bytes = standard_signature_bytes.to_vec();
    signature_bytes.insert(0, recovery_id.to_i32() as u8);

    let p2tr_output = TxOut {
        value: amount,
        script_pubkey: p2tr_script.clone(),
    };

    let p2tr_output_2 = TxOut {
        value: fulfillment_fee,
        script_pubkey: p2tr_script,
    };

    let mut op_bytes = vec![105, 100, '>' as u8];
    op_bytes.extend_from_slice(&amount.to_be_bytes());
    op_bytes.extend_from_slice(&signature_bytes);

    let op_return_script = Builder::new()
        .push_opcode(opcodes::all::OP_RETURN)
        .push_slice(&op_bytes)
        .into_script();
    let op_return_output = TxOut {
        value: 0,
        script_pubkey: op_return_script,
    };
    tx.output.push(op_return_output);
    tx.output.push(p2tr_output);
    tx.output.push(p2tr_output_2);

    let serialized_tx = serialize(&tx);
    let hex_tx = array_bytes::bytes2hex("", &serialized_tx);

    println!("Transaction: {:?}", tx);
    println!("Serialized transaction: {:?}", serialized_tx);
    println!("Hex tx: {}", hex_tx);
}
