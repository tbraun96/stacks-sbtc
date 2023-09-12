use bitcoin::consensus::{Decodable, Encodable};
use bitcoin::schnorr::TweakedPublicKey;
use bitcoin::secp256k1::{self, All, Message, Secp256k1, SecretKey};
use bitcoin::util::sighash::SighashCache;
use bitcoin::{
    Address, EcdsaSighashType, KeyPair, Network, OutPoint, PackedLockTime, PrivateKey, PublicKey,
    SchnorrSighashType, Script, Sequence, Transaction, TxOut, Txid, Witness, XOnlyPublicKey,
};
use hashbrown::HashMap;
use libc::pid_t;
use rand::rngs::OsRng;
use std::{
    env,
    fmt::Debug,
    fs::{create_dir, remove_dir_all},
    net::TcpListener,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    str::FromStr,
    time::{Duration, SystemTime},
};
use url::Url;
use wsts::{
    common::PolyCommitment,
    compute::tweaked_public_key,
    taproot::{
        test_helpers::{dkg, sign},
        SchnorrProof,
    },
    v1::{SignatureAggregator, Signer},
    Point,
};

use nix::sys::signal::{self, Signal};
use nix::unistd::Pid;

use once_cell::sync::Lazy;
use reqwest::StatusCode;
use serde::Serialize;
use serde_json::{json, Value};
use std::sync::Mutex;
use uuid::Uuid;

const BITCOIND_URL: &str = "http://abcd:abcd@localhost";
const MIN_PORT: u16 = 20000;
const MAX_PORT: u16 = 25000;

// Lazily instantiate a static port factory that will only supply ports that haven't already been
// claimed.
//
// Note: This structure will be insufficient if multiple rust files assign ports.
static CLAIMED_PORT_FACTORY: Mutex<Lazy<ClaimedPortFactory>> =
    Mutex::new(Lazy::new(ClaimedPortFactory::default));

/// A structure intended to be a static singleton that claims ports to avoid collisions in other threads.
#[derive(Default)]
pub struct ClaimedPortFactory {
    /// Structure tracking claimed ports
    claimedports: HashMap<u16, Uuid>,
}

impl ClaimedPortFactory {
    /// Attempts to claim a port in the specified range.
    ///
    /// Returns `None` if no port can be successfully claimed.
    pub fn claim_port_in_range(
        &mut self,
        claimant: Uuid,
        minport: u16,
        maxport: u16,
    ) -> Option<u16> {
        (minport..=maxport).find(|port| {
            Self::port_is_open(*port)
                .map(|_listener| self.claimedports.try_insert(*port, claimant).is_ok())
                .unwrap_or_default()
        })
    }

    /// Drops all ports claimed by the specified claimant.
    pub fn drop_all_ports_for_claimant(&mut self, claimant: Uuid) {
        self.claimedports.retain(|_port, uuid| !claimant.eq(uuid))
    }

    /// Returns `true` if the specified port is open.
    fn port_is_open(port: u16) -> Option<TcpListener> {
        TcpListener::bind(("127.0.0.1", port)).ok()
    }
}

pub struct Process {
    pub datadir: PathBuf,
    pub child: Child,
}

impl Process {
    pub fn new(cmd: &str, args: &[&str], envs: &HashMap<String, String>) -> Self {
        // Create unique test id to track assets for this process.
        let testid: Uuid = Uuid::new_v4();

        let mut datadir: PathBuf = PathBuf::from_str("/tmp/").unwrap();
        let tempfile: String = format!("test_utils_{}", testid);

        datadir = datadir.join(tempfile);
        create_dir(&datadir).unwrap();

        let child = Self::spawn(cmd, args, envs);

        Process { datadir, child }
    }

    fn spawn(cmd: &str, args: &[&str], envs: &HashMap<String, String>) -> Child {
        let child = Command::new(cmd)
            .envs(envs)
            .args(args)
            .stdout(Stdio::inherit())
            .spawn()
            .unwrap_or_else(|_| panic!("{} failed to start", cmd));

        let pid = child.id() as pid_t;

        // Attempt to set a ctrlc handler if it hasn't been set yet
        let _ = ctrlc::set_handler(move || {
            println!("Killing pid {:?}...", pid);

            signal::kill(Pid::from_raw(pid), Signal::SIGTERM)
                .map_err(|e| println!("Warning: signaling pid {} failed {:?}", pid, e))
                .unwrap();
        });

        child
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        match self.child.kill() {
            Ok(_) => (),
            Err(e) => {
                println!("Failed to kill pid {}: {:?}", self.child.id(), e);
            }
        }
        remove_dir_all(&self.datadir).unwrap();
    }
}

pub struct BitcoinProcess {
    testid: Uuid,
    url: Url,
    datadir: PathBuf,
    child: Child,
}

impl BitcoinProcess {
    fn spawn(port: u16, rpcport: u16, datadir: &Path) -> Child {
        // Spin up the bitcoind command line program.
        let bitcoind_child = Command::new("bitcoind")
            .arg("-regtest")
            .arg("-bind=0.0.0.0")
            .arg("-rpcuser=abcd")
            .arg("-rpcpassword=abcd")
            .arg(format!("-port={}", port))
            .arg(format!("-rpcport={}", rpcport))
            .arg(format!("-datadir={}", datadir.to_str().unwrap()))
            .stdout(Stdio::null())
            .spawn()
            .expect("bitcoind failed to start");

        let bitcoind_pid = bitcoind_child.id() as pid_t;

        // Attempt to set a ctrlc handler if it hasn't been set yet
        let _ = ctrlc::set_handler(move || {
            println!("Killing bitcoind pid {:?}...", bitcoind_pid);

            signal::kill(Pid::from_raw(bitcoind_pid), Signal::SIGTERM)
                .map_err(|e| {
                    println!(
                        "Warning: signaling bitcoind {} failed {:?}",
                        bitcoind_pid, e
                    )
                })
                .unwrap();
        });

        bitcoind_child
    }

    pub async fn rpc(&self, method: &str, params: impl Serialize) -> Value {
        let rpc = json!({"jsonrpc": "1.0", "id": "tst", "method": method, "params": params});

        match reqwest::Client::new()
            .post(self.url.as_str())
            .json(&rpc)
            .send()
            .await
        {
            Ok(response) => {
                let status = response.status();
                let json = response.json::<Value>().await.unwrap();

                if status != StatusCode::OK {
                    return json!({ "error": json.to_string() });
                }

                let result = json.as_object().unwrap().get("result").unwrap().clone();
                result
            }
            Err(err) => {
                let err_str = err.to_string();
                json!({ "error": &err_str })
            }
        }
    }

    async fn connectivity_check(&self) -> Result<f32, String> {
        let now = SystemTime::now();

        for _tries in 1..120 {
            let uptime = self.rpc("uptime", ()).await;

            if uptime.is_number() {
                return Ok(now.elapsed().unwrap().as_secs_f32());
            } else {
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
        }

        Err("connection timeout".to_string())
    }

    pub async fn new() -> Self {
        // Create unique test id to track assets for this process.
        let testid: Uuid = Uuid::new_v4();

        // Claim ports.
        let port: Option<u16> = CLAIMED_PORT_FACTORY
            .lock()
            .unwrap()
            .claim_port_in_range(testid, MIN_PORT, MAX_PORT);
        let rpcport: Option<u16> = CLAIMED_PORT_FACTORY
            .lock()
            .unwrap()
            .claim_port_in_range(testid, MIN_PORT, MAX_PORT);

        // Generate url.
        let mut url: Url = BITCOIND_URL.parse().unwrap();
        url.set_port(rpcport).unwrap();

        // Create temp directory for tests.
        let mut datadir: PathBuf = PathBuf::from_str("/tmp/").unwrap();
        let tempfile: String = format!("test_utils_{}", testid);

        datadir = datadir.join(tempfile);
        create_dir(&datadir).unwrap();

        let child = Self::spawn(port.unwrap(), rpcport.unwrap(), &datadir);

        let this = Self {
            testid,
            url,
            datadir,
            child,
        };
        this.connectivity_check().await.unwrap();

        this
    }

    pub fn url(&self) -> &Url {
        &self.url
    }
}

impl Drop for BitcoinProcess {
    fn drop(&mut self) {
        self.child.kill().unwrap();
        remove_dir_all(&self.datadir).unwrap();
        CLAIMED_PORT_FACTORY
            .lock()
            .unwrap()
            .drop_all_ports_for_claimant(self.testid);
    }
}

pub fn generate_wallet(
    is_taproot: bool,
) -> (
    SecretKey,
    PrivateKey,
    PublicKey,
    XOnlyPublicKey,
    Address,
    Secp256k1<All>,
) {
    let secp = Secp256k1::new();
    let keypair = KeyPair::new(&secp, &mut rand::thread_rng());

    let secret_key = keypair.secret_key();
    let private_key: PrivateKey = PrivateKey::new(secret_key, Network::Regtest);
    let public_key = PublicKey::from_private_key(&secp, &private_key);
    let xonly_public_key = keypair.x_only_public_key().0;
    let address = if is_taproot {
        let tweaked_public_key = TweakedPublicKey::dangerous_assume_tweaked(xonly_public_key);
        Address::p2tr_tweaked(tweaked_public_key, Network::Regtest)
    } else {
        Address::p2wpkh(&public_key, Network::Regtest).unwrap()
    };
    (
        secret_key,
        private_key,
        public_key,
        xonly_public_key,
        address,
        secp,
    )
}

pub async fn mine_and_get_coinbase_txid(btcd: &BitcoinProcess, addr: &Address) -> (Txid, String) {
    let block_id = btcd
        .rpc("generatetoaddress", (100, addr.to_string()))
        .await
        .as_array()
        .unwrap()[0]
        .as_str()
        .unwrap()
        .to_string();

    let block = btcd.rpc("getblock", (block_id, 1)).await;
    let blockhash = block.get("hash").unwrap().as_str().unwrap().to_string();

    (
        Txid::from_str(block.get("tx").unwrap().get(0).unwrap().as_str().unwrap()).unwrap(),
        blockhash,
    )
}

pub async fn get_raw_transaction(
    btcd: &BitcoinProcess,
    txid: &Txid,
    blockhash: Option<String>,
) -> Result<Transaction, bitcoin::consensus::encode::Error> {
    let tx_raw = if let Some(blockhash) = blockhash {
        btcd.rpc("getrawtransaction", (&txid.to_string(), false, blockhash))
            .await
            .as_str()
            .unwrap()
            .to_string()
    } else {
        btcd.rpc("getrawtransaction", (&txid.to_string(), false))
            .await
            .as_str()
            .unwrap()
            .to_string()
    };
    Transaction::consensus_decode(&mut hex::decode(tx_raw).unwrap().as_slice())
}

/// A helper struct for executing DKG rounds and generating Schnorr signatures
pub struct SignerHelper {
    threshold: u32,
    total: u32,
    rng: OsRng,
    signers: [Signer; 3],
}

impl Default for SignerHelper {
    fn default() -> Self {
        // Signer setup
        let threshold = 3;
        let total = 4;
        let mut rng = OsRng;
        let signers = [
            Signer::new(1, &[0, 1], total, threshold, &mut rng),
            Signer::new(2, &[2], total, threshold, &mut rng),
            Signer::new(3, &[3], total, threshold, &mut rng),
        ];

        Self {
            threshold,
            total,
            rng,
            signers,
        }
    }
}

impl SignerHelper {
    pub fn run_distributed_key_generation(
        &mut self,
        merkle_root: Option<[u8; 32]>,
    ) -> (Vec<PolyCommitment>, Point, bitcoin::PublicKey) {
        // DKG (Distributed Key Generation)

        let public_commitments = dkg(&mut self.signers, &mut self.rng, merkle_root)
            .expect("Failed to run distributed key generation.");
        let group_public_key_point = public_commitments
            .iter()
            .fold(Point::new(), |s, poly| s + poly.A[0]);
        let tweaked_group_public_key_point =
            tweaked_public_key(&group_public_key_point, merkle_root);

        let group_public_key =
            bitcoin::PublicKey::from_slice(tweaked_group_public_key_point.compress().as_bytes())
                .expect("Failed to create public key from DKG result.");

        (
            public_commitments,
            tweaked_group_public_key_point,
            group_public_key,
        )
    }

    pub fn signing_round(
        &mut self,
        message: &[u8],
        public_commitments: Vec<PolyCommitment>,
        merkle_root: Option<[u8; 32]>,
    ) -> SchnorrProof {
        // decide which signers will be used
        let mut signers = [self.signers[0].clone(), self.signers[1].clone()];

        let (nonces, shares) = sign(message, &mut signers, &mut self.rng, merkle_root);

        let mut agg = SignatureAggregator::new(self.total, self.threshold, public_commitments)
            .expect("Failed to create signature aggregator.");
        let sig = agg
            .sign_taproot(message, &nonces, &shares, merkle_root)
            .expect("Failed to create signature.");

        SchnorrProof::new(&sig).expect("Failed to create Schnorr proof.")
    }
}

pub fn sign_transaction_ecdsa(
    addr: &Address,
    secret_key: &SecretKey,
    public_key: &PublicKey,
    prev_output: &TxOut,
    tx: &mut Transaction,
    secp: &Secp256k1<All>,
) -> String {
    let tx_sighash_pubkey_script = addr.script_pubkey().p2wpkh_script_code().unwrap();
    let mut sighash_cache_peg_in = SighashCache::new(&*tx);

    let tx_sighash = sighash_cache_peg_in
        .segwit_signature_hash(
            0,
            &tx_sighash_pubkey_script,
            prev_output.value,
            EcdsaSighashType::All,
        )
        .unwrap();

    let msg = Message::from_slice(&tx_sighash).unwrap();
    let sig = secp.sign_ecdsa_low_r(&msg, secret_key);
    let secp_public_key_source = secp256k1::PublicKey::from_secret_key(secp, secret_key);

    secp.verify_ecdsa(&msg, &sig, &secp_public_key_source)
        .unwrap();

    tx.input[0]
        .witness
        .push_bitcoin_signature(&sig.serialize_der(), EcdsaSighashType::All);
    tx.input[0]
        .witness
        .push(bitcoin::psbt::serialize::Serialize::serialize(public_key));

    let mut tx_bytes: Vec<u8> = vec![];
    let _tx_bytes_len = tx.consensus_encode(&mut tx_bytes).unwrap();
    let tx_bytes_hex = hex::encode(&tx_bytes);

    println!("tx bytes {}", &tx_bytes_hex);
    tx_bytes_hex
}

pub fn sign_transaction_taproot(
    tx: &mut Transaction,
    prev_output: &TxOut,
    signer: &mut SignerHelper,
    group_public_key: &Point,
    public_commitments: Vec<PolyCommitment>,
    merkle_root: Option<[u8; 32]>,
) -> String {
    let mut sighash_cache = bitcoin::util::sighash::SighashCache::new(&*tx);
    let taproot_sighash = sighash_cache
        .taproot_key_spend_signature_hash(
            0,
            &bitcoin::util::sighash::Prevouts::All(&[prev_output]),
            SchnorrSighashType::Default,
        )
        .unwrap();
    let signing_payload = taproot_sighash.as_hash().to_vec();
    // signing. Signers: 0 (parties: 0, 1) and 1 (parties: 2)
    let schnorr_proof = signer.signing_round(&signing_payload, public_commitments, merkle_root);
    assert!(schnorr_proof.verify(&group_public_key.x(), &signing_payload));

    let mut frost_sig_bytes = vec![];
    frost_sig_bytes.extend(schnorr_proof.r.to_bytes());
    frost_sig_bytes.extend(schnorr_proof.s.to_bytes());

    tx.input[0].witness.push(&frost_sig_bytes);
    let mut tx_bytes: Vec<u8> = vec![];
    let _tx_bytes_len = tx.consensus_encode(&mut tx_bytes).unwrap();
    let tx_bytes_hex = hex::encode(&tx_bytes);

    println!("tx bytes {}", &tx_bytes_hex);
    tx_bytes_hex
}

/// Build a transaction that deposits funds into the specified deposit wallet
pub fn build_transaction_deposit(
    satoshis: u64,
    deposit_wallet_public_key: bitcoin::PublicKey,
    stx_address: [u8; 32],
    prev_output: OutPoint,
) -> Transaction {
    let deposit_input = bitcoin::TxIn {
        previous_output: prev_output,
        script_sig: Default::default(),
        sequence: Sequence::MAX,
        witness: Witness::new(),
    };

    // Build a SIP-21 consensus compatible deposit transaction
    let mut sip_21_deposit_data = vec![0, 0, b'<'];
    sip_21_deposit_data.extend_from_slice(&stx_address);

    let op_return = Script::new_op_return(&sip_21_deposit_data);
    let deposit_output_0 = bitcoin::TxOut {
        value: 0,
        script_pubkey: op_return,
    };

    // crate type weirdness
    let deposit_wallet_public_key_secp =
        bitcoin::secp256k1::PublicKey::from_slice(&deposit_wallet_public_key.to_bytes()).unwrap();
    let deposit_wallet_public_key_xonly = XOnlyPublicKey::from(deposit_wallet_public_key_secp);

    // Do not want to use Script::new_v1_p2tr because it will tweak our key when we don't want it to
    let deposit_wallet_public_key_tweaked =
        TweakedPublicKey::dangerous_assume_tweaked(deposit_wallet_public_key_xonly);
    let taproot_script = Script::new_v1_p2tr_tweaked(deposit_wallet_public_key_tweaked);

    let deposit_output_1 = bitcoin::TxOut {
        value: satoshis,
        script_pubkey: taproot_script,
    };
    Transaction {
        version: 2, // Must use version 2 to be BIP-341 compatible
        lock_time: PackedLockTime(0),
        input: vec![deposit_input],
        output: vec![deposit_output_0, deposit_output_1],
    }
}

/// Build a transaction that spends the utxo to the specified public_key
pub fn build_transaction_withdrawal(
    satoshis: u64,
    public_key: bitcoin::PublicKey,
    utxo: OutPoint,
) -> Transaction {
    let withdrawal_input = bitcoin::TxIn {
        previous_output: utxo,
        script_sig: Default::default(),
        sequence: Default::default(),
        witness: Default::default(),
    };
    let p2wpk = Script::new_v0_p2wpkh(&public_key.wpubkey_hash().unwrap());
    let withdrawal_output = bitcoin::TxOut {
        value: satoshis,
        script_pubkey: p2wpk,
    };
    bitcoin::blockdata::transaction::Transaction {
        version: 2,
        lock_time: PackedLockTime(0),
        input: vec![withdrawal_input],
        output: vec![withdrawal_output],
    }
}

pub fn parse_env<T: FromStr>(var: &str, default: T) -> T
where
    <T as FromStr>::Err: Debug,
{
    match env::var(var) {
        Ok(var) => var.parse::<T>().unwrap(),
        Err(_) => default,
    }
}
