use libc::pid_t;
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

use ctrlc;
use ctrlc::Signal;
use nix::sys::signal;
use nix::unistd::Pid;
use ureq;
use ureq::serde_json::Value;
use ureq::{json, serde_json};

const BITCOIND_URL: &str = "http://abcd:abcd@localhost:18443";

pub fn bitcoind_rpc(method: &str, params: impl ureq::serde::Serialize) -> serde_json::Value {
    let rpc = ureq::json!({"jsonrpc": "1.0", "id": "tst", "method": method, "params": params});
    match ureq::post(BITCOIND_URL).send_json(&rpc) {
        Ok(response) => {
            let json = response.into_json::<serde_json::Value>().unwrap();
            let result = json.as_object().unwrap().get("result").unwrap().clone();
            result
        }
        Err(err) => {
            let err_str = err.to_string();
            let err_obj_opt = match err.into_response() {
                Some(r) => r.into_json::<serde_json::Value>().unwrap(),
                None => json!({ "error": &err_str }),
            };
            println!("{} -> {}", rpc, err_obj_opt);
            err_obj_opt
        }
    }
}

pub fn bitcoind_setup() -> BitcoinPid {
    let bitcoind_child = Command::new("bitcoind")
        .arg("-regtest")
        .arg("-rpcuser=abcd")
        .arg("-rpcpassword=abcd")
        .stdout(Stdio::null())
        .spawn()
        .expect("bitcoind failed to start");
    let bitcoind_pid = bitcoind_child.id() as pid_t;
    ctrlc::set_handler(move || {
        println!("kill bitcoind pid {:?}", bitcoind_pid);
        stop_pid(bitcoind_pid)
    })
    .expect("Error setting Ctrl-C handler");
    match connectivity_check() {
        Err(e) => {
            panic!("no bitcoind available! {e}");
        }
        Ok(elapsed) => {
            println!(
                "bitconind pid {} started. warmed up in {} seconds",
                bitcoind_pid, elapsed
            );
            BitcoinPid::new(bitcoind_pid)
        }
    }
}

pub fn connectivity_check() -> Result<f32, String> {
    let now = std::time::SystemTime::now();
    for _tries in 1..120 {
        let uptime = bitcoind_rpc("uptime", ());
        if uptime.is_number() {
            return Ok(now.elapsed().unwrap().as_secs_f32());
        } else {
            thread::sleep(Duration::from_millis(500));
        }
    }
    Err("connection timeout".to_string())
}

pub fn bitcoind_mine(public_key_bytes: &[u8; 33]) -> Value {
    let public_key = bitcoin::PublicKey::from_slice(public_key_bytes).unwrap();
    let address = bitcoin::Address::p2wpkh(&public_key, bitcoin::Network::Regtest).unwrap();
    bitcoind_rpc("generatetoaddress", (128, address.to_string()))
}

pub fn stop_pid(pid: pid_t) {
    signal::kill(Pid::from_raw(pid), Signal::SIGTERM)
        .map_err(|e| println!("warning: signaling bitcoind {} failed {:?}", pid, e))
        .unwrap();
}

pub struct BitcoinPid {
    pid: pid_t,
}

impl BitcoinPid {
    fn new(pid: pid_t) -> Self {
        BitcoinPid { pid }
    }
}

impl Drop for BitcoinPid {
    fn drop(&mut self) {
        println!("bitcoind {} stopping", self.pid);
        stop_pid(self.pid);
    }
}
