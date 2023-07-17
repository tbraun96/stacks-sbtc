use std::collections::{BTreeMap, HashMap};
use std::io::stdout;
use std::iter::once;
use std::str::FromStr;

use anyhow::{anyhow, Context};
use array_bytes::bytes2hex;
use bdk::blockchain::Blockchain;
use bdk::keys::bip39::Mnemonic;
use bdk::keys::{DerivableKey, ExtendedKey};
use bdk::miniscript::BareCtx;
use bdk::SignOptions;
use bdk::{
    blockchain::ElectrumBlockchain, database::MemoryDatabase, electrum_client::Client,
    template::P2Wpkh, SyncOptions, Wallet,
};
use bitcoin::blockdata::opcodes;
use bitcoin::blockdata::script::Builder;
use bitcoin::psbt::serialize::{Deserialize, Serialize};
use bitcoin::psbt::PartiallySignedTransaction;
use bitcoin::schnorr::TweakedPublicKey;
use bitcoin::secp256k1::rand::random;
use bitcoin::secp256k1::{Message, Secp256k1};
use bitcoin::{Address as BitcoinAddress, Network, PrivateKey, TxOut};
use bitcoin::{Script, Transaction};
use blockstack_lib::address::{
    C32_ADDRESS_VERSION_MAINNET_SINGLESIG, C32_ADDRESS_VERSION_TESTNET_SINGLESIG,
};
use blockstack_lib::types::{chainstate::StacksAddress, Address};
use blockstack_lib::util::hash::Hash160;
use clap::Parser;

fn main() -> Result<(), anyhow::Error> {
    let args = Cli::parse();

    match args.command {
        Command::Deposit(deposit_args) => build_deposit_tx(&deposit_args),
        Command::Withdraw(withdrawal_args) => build_withdrawal_tx(&withdrawal_args),
        Command::Broadcast(broadcast_args) => broadcast_tx(&broadcast_args),
        Command::GenerateFrom(generate_args) => generate(&generate_args),
    }
}

#[derive(clap::Subcommand, Debug, Clone)]
enum Command {
    Deposit(DepositArgs),
    Withdraw(WithdrawalArgs),
    Broadcast(BroadcastArgs),
    GenerateFrom(GenerateArgs),
}

#[derive(Parser, Debug, Clone)]
struct DepositArgs {
    /// P2WPKH BTC private key in WIF format
    #[clap(short, long)]
    wif: String,

    /// Stacks address that will receive sBTC
    #[clap(short, long)]
    recipient: String,

    /// The amount of sats to send
    #[clap(short, long)]
    amount: u64,

    /// Dkg wallet address
    #[clap(short, long)]
    dkg_wallet: String,
}

#[derive(Parser, Debug, Clone)]
struct WithdrawalArgs {
    /// P2WPKH BTC private key in WIF format
    #[clap(short, long)]
    wif: String,

    /// P2WPKH sBTC sender private key in WIF format
    #[clap(short, long)]
    sender_wif: String,

    /// Bitcoin address that will receive BTC
    #[clap(short, long)]
    recipient: String,

    /// The amount of sats to send
    #[clap(short, long)]
    amount: u64,

    /// The amount of sats to send as the fulfillment fee
    #[clap(short, long)]
    fulfillment_fee: u64,

    /// Dkg wallet address
    #[clap(short, long)]
    dkg_wallet: String,
}

#[derive(Parser, Debug, Clone)]
struct BroadcastArgs {
    /// The network to broadcast to
    #[clap(short, long, default_value_t = Network::Testnet)]
    network: Network,
    /// The transaction to broadcast
    tx: String,
}

#[derive(clap::Subcommand, Debug, Clone)]
enum GenerateSubcommand {
    New,
    Wif { wif: String },
    PrivateKeyHex { private_key: String },
    Mnemonic { mnemonic: String },
}

#[derive(clap::Parser, Debug, Clone)]
struct GenerateArgs {
    /// Specify how to generate the credentials
    #[command(subcommand)]
    subcommand: GenerateSubcommand,
    /// The network to broadcast to
    #[clap(short, long, default_value_t = Network::Testnet)]
    network: Network,
}

#[derive(Parser)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

fn init_blockchain() -> anyhow::Result<ElectrumBlockchain> {
    let client = Client::new("ssl://blockstream.info:993")?;
    let blockchain = ElectrumBlockchain::from(client);
    Ok(blockchain)
}

fn build_deposit_tx(deposit: &DepositArgs) -> anyhow::Result<()> {
    let private_key = PrivateKey::from_wif(&deposit.wif)?;

    let wallet = setup_wallet(private_key)?;

    let recipient = StacksAddress::from_string(&deposit.recipient)
        .ok_or(anyhow::anyhow!("Could not parse recipient Stacks address"))?;
    let dkg_address = BitcoinAddress::from_str(&deposit.dkg_wallet)?;

    let mut psbt = deposit_psbt(
        &wallet,
        &recipient,
        &dkg_address,
        deposit.amount,
        &private_key.network,
    )?;

    wallet.sign(&mut psbt, SignOptions::default())?;
    let tx = psbt.extract_tx();
    println!("Resulting deposit txid: {}", tx.txid());
    println!(
        "Resulting serialized deposit tx:\n{}",
        array_bytes::bytes2hex("", tx.serialize())
    );

    Ok(())
}

fn build_withdrawal_tx(withdrawal: &WithdrawalArgs) -> anyhow::Result<()> {
    let private_key = PrivateKey::from_wif(&withdrawal.wif)?;

    let wallet = setup_wallet(private_key)?;

    let sender_private_key = PrivateKey::from_wif(&withdrawal.sender_wif)?;
    let recipient = BitcoinAddress::from_str(&withdrawal.recipient)?;
    let dkg_address = BitcoinAddress::from_str(&withdrawal.dkg_wallet)?;

    let mut psbt = withdrawal_psbt(
        &wallet,
        &sender_private_key,
        &recipient,
        &dkg_address,
        withdrawal.amount,
        withdrawal.fulfillment_fee,
        &private_key.network,
    )?;

    wallet.sign(&mut psbt, SignOptions::default())?;
    let tx = psbt.extract_tx();
    println!("Resulting withdrawal txid: {}", tx.txid());
    println!(
        "Resulting serialized withdrawal tx:\n{}",
        array_bytes::bytes2hex("", tx.serialize())
    );
    Ok(())
}

fn broadcast_tx(broadcast: &BroadcastArgs) -> anyhow::Result<()> {
    let blockchain = init_blockchain()?;
    let tx = Transaction::deserialize(
        &array_bytes::hex2bytes(&broadcast.tx).map_err(|e| anyhow!("{:?}", e))?,
    )?;
    blockchain.broadcast(&tx)?;

    println!("Broadcasted tx: {}", tx.txid());
    Ok(())
}

#[derive(serde::Serialize, Debug, Clone)]
struct Credentials {
    mnemonic: String,
    wif: String,
    private_key: String,
    public_key: String,
    stacks_address: String,
    bitcoin_taproot_address_tweaked: String,
    bitcoin_taproot_address_untweaked: String,
    bitcoin_p2pkh_address: String,
}

fn generate(generate_args: &GenerateArgs) -> anyhow::Result<()> {
    let (private_key, maybe_mnemonic) = match &generate_args.subcommand {
        GenerateSubcommand::New => {
            let mnemonic = random_mnemonic()?;
            (
                private_key_from_mnemonic(generate_args.network, mnemonic.clone())?,
                Some(mnemonic),
            )
        }
        GenerateSubcommand::Wif { wif } => (private_key_from_wif(wif)?, None),
        GenerateSubcommand::PrivateKeyHex { private_key } => (
            parse_private_key_from_hex(private_key, generate_args.network)?,
            None,
        ),
        GenerateSubcommand::Mnemonic { mnemonic } => {
            let mnemonic = Mnemonic::parse(mnemonic)?;
            (
                private_key_from_mnemonic(generate_args.network, mnemonic.clone())?,
                Some(mnemonic),
            )
        }
    };

    let credentials = generate_credentials(&private_key, maybe_mnemonic)?;

    serde_json::to_writer_pretty(stdout(), &credentials)?;

    Ok(())
}

fn random_mnemonic() -> anyhow::Result<Mnemonic> {
    let entropy: Vec<u8> = std::iter::from_fn(|| Some(random())).take(32).collect();
    Mnemonic::from_entropy(&entropy).context("Could not create mnemonic from entropy")
}

fn private_key_from_wif(wif: &str) -> anyhow::Result<PrivateKey> {
    Ok(PrivateKey::from_wif(wif)?)
}

fn parse_private_key_from_hex(private_key: &str, network: Network) -> anyhow::Result<PrivateKey> {
    let slice = array_bytes::hex2bytes(private_key)
        .map_err(|_| anyhow::anyhow!("Failed to parse hex string: {}", private_key,))?;
    Ok(PrivateKey::from_slice(&slice, network)?)
}

fn private_key_from_mnemonic(network: Network, mnemonic: Mnemonic) -> anyhow::Result<PrivateKey> {
    let extended_key: ExtendedKey<BareCtx> = mnemonic.into_extended_key()?;
    let private_key = extended_key
        .into_xprv(network)
        .ok_or(anyhow!("Could not create an extended private key"))?;

    Ok(private_key.to_priv())
}

fn generate_credentials(
    private_key: &PrivateKey,
    maybe_mnemonic: Option<Mnemonic>,
) -> anyhow::Result<Credentials> {
    let secp = Secp256k1::new();
    let public_key = private_key.public_key(&secp);

    let stacks_address_version = match private_key.network {
        Network::Testnet => C32_ADDRESS_VERSION_TESTNET_SINGLESIG,
        Network::Bitcoin => C32_ADDRESS_VERSION_MAINNET_SINGLESIG,
        _ => panic!("Not supported"),
    };
    let public_key_hash = Hash160::from_vec(&public_key.pubkey_hash().as_hash().to_vec()).unwrap();
    let stacks_address = StacksAddress::new(stacks_address_version, public_key_hash);
    let bitcoin_taproot_address_tweaked =
        BitcoinAddress::p2tr(&secp, public_key.inner.into(), None, private_key.network).to_string();

    let bitcoin_taproot_address_untweaked = BitcoinAddress::p2tr_tweaked(
        TweakedPublicKey::dangerous_assume_tweaked(public_key.inner.into()),
        private_key.network,
    )
    .to_string();

    Ok(Credentials {
        mnemonic: maybe_mnemonic
            .as_ref()
            .map(ToString::to_string)
            .unwrap_or_default(),
        wif: private_key.to_wif(),
        private_key: bytes2hex("0x", private_key.to_bytes()),
        public_key: bytes2hex("0x", public_key.to_bytes()),
        stacks_address: stacks_address.to_string(),
        bitcoin_taproot_address_tweaked,
        bitcoin_taproot_address_untweaked,
        bitcoin_p2pkh_address: BitcoinAddress::p2pkh(&public_key, private_key.network).to_string(),
    })
}

fn setup_wallet(private_key: PrivateKey) -> anyhow::Result<Wallet<MemoryDatabase>> {
    let blockchain = init_blockchain()?;
    let wallet = Wallet::new(
        P2Wpkh(private_key),
        Some(P2Wpkh(private_key)),
        private_key.network,
        MemoryDatabase::default(),
    )?;

    wallet.sync(&blockchain, SyncOptions::default())?;

    Ok(wallet)
}

fn deposit_psbt(
    wallet: &Wallet<MemoryDatabase>,
    recipient: &StacksAddress,
    dkg_address: &BitcoinAddress,
    amount: u64,
    network: &Network,
) -> anyhow::Result<PartiallySignedTransaction> {
    let mut tx_builder = wallet.build_tx();

    let op_return_script = build_op_return_script(&deposit_data(recipient, network));
    let dkg_script = dkg_address.script_pubkey();
    let dust_amount = dkg_script.dust_value().to_sat();

    if amount < dust_amount {
        return Err(anyhow!(
            "Provided amount {} is less than the dust amount: {}",
            amount,
            dust_amount
        ));
    }

    let outputs = [(op_return_script, 0), (dkg_script, amount)];

    for (script, amount) in outputs.clone() {
        tx_builder.add_recipient(script, amount);
    }

    let (mut partial_tx, _) = tx_builder.finish()?;

    partial_tx.unsigned_tx.output =
        reorder_outputs(partial_tx.unsigned_tx.output.into_iter(), outputs);

    Ok(partial_tx)
}

fn withdrawal_psbt(
    wallet: &Wallet<MemoryDatabase>,
    sender_private_key: &PrivateKey,
    recipient: &BitcoinAddress,
    dkg_address: &BitcoinAddress,
    amount: u64,
    fulfillment_fee: u64,
    network: &Network,
) -> anyhow::Result<PartiallySignedTransaction> {
    let recipient_script = recipient.script_pubkey();
    let dkg_wallet_script = dkg_address.script_pubkey();

    // Check that we have enough to cover dust
    let recipient_dust_amount = recipient_script.dust_value().to_sat();
    let dkg_wallet_dust_amount = dkg_wallet_script.dust_value().to_sat();

    if fulfillment_fee < dkg_wallet_dust_amount {
        return Err(anyhow!(
            "Provided fulfillment fee {} is less than the dust amount: {}",
            fulfillment_fee,
            dkg_wallet_dust_amount
        ));
    }

    let op_return_script = build_op_return_script(&withdrawal_data(
        recipient,
        amount,
        sender_private_key,
        network,
    ));

    let mut tx_builder = wallet.build_tx();

    let outputs = [
        (op_return_script, 0),
        (recipient_script, recipient_dust_amount),
        (dkg_wallet_script, fulfillment_fee),
    ];

    for (script, amount) in outputs.clone() {
        tx_builder.add_recipient(script, amount);
    }

    let (mut partial_tx, _) = tx_builder.finish()?;

    partial_tx.unsigned_tx.output =
        reorder_outputs(partial_tx.unsigned_tx.output.into_iter(), outputs);

    Ok(partial_tx)
}

fn build_op_return_script(data: &[u8]) -> Script {
    Builder::new()
        .push_opcode(opcodes::all::OP_RETURN)
        .push_slice(data)
        .into_script()
}

fn withdrawal_data(
    recipient: &BitcoinAddress,
    amount: u64,
    sender_private_key: &PrivateKey,
    network: &Network,
) -> Vec<u8> {
    let mut msg = amount.to_be_bytes().to_vec();
    msg.extend_from_slice(recipient.script_pubkey().as_bytes());

    let msg_hash = sha256::digest(msg.as_slice());
    let msg_hash_bytes = array_bytes::hex2bytes(msg_hash).unwrap();
    let msg_ecdsa = Message::from_slice(&msg_hash_bytes).unwrap();

    let (recovery_id, signature) = Secp256k1::new()
        .sign_ecdsa_recoverable(&msg_ecdsa, &sender_private_key.inner)
        .serialize_compact();

    magic_bytes(network)
        .into_iter()
        .chain(once(b'>'))
        .chain(amount.to_be_bytes())
        .chain(once(recovery_id.to_i32() as u8))
        .chain(signature)
        .collect()
}

fn deposit_data(recipient: &StacksAddress, network: &Network) -> Vec<u8> {
    magic_bytes(network)
        .into_iter()
        .chain(once(b'<'))
        .chain(once(recipient.version))
        .chain(recipient.to_bytes())
        .collect()
}

fn magic_bytes(network: &Network) -> [u8; 2] {
    match network {
        Network::Bitcoin => [b'X', b'2'],
        Network::Testnet => [b'T', b'2'],
        _ => [b'i', b'd'],
    }
}

fn reorder_outputs(
    outputs: impl IntoIterator<Item = TxOut>,
    order: impl IntoIterator<Item = (Script, u64)>,
) -> Vec<TxOut> {
    let indices: HashMap<(Script, u64), usize> = order
        .into_iter()
        .enumerate()
        .map(|(idx, val)| (val, idx))
        .collect();

    let outputs_ordered: BTreeMap<usize, TxOut> = outputs
        .into_iter()
        .map(|txout| {
            (
                *indices
                    .get(&(txout.script_pubkey.clone(), txout.value))
                    .unwrap_or(&usize::MAX), // Change amount
                txout,
            )
        })
        .collect();

    outputs_ordered.into_values().collect()
}
