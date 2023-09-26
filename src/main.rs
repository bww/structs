use std::io;
use std::fs;
use std::env;
use std::str;
use std::path;
use std::process;
use std::io::Read;

use std::thread;
use std::os::unix::net::{UnixStream, UnixListener};
use std::sync::mpsc;
use ctrlc;

use std::collections::BTreeMap;
use rand::distributions::{Alphanumeric, DistString};

use colored::Colorize;
use clap::{Parser, Subcommand, Args};

use serde_json;

mod error;
mod service;
mod rpc;

const _VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Options {
  #[clap(long, help="Enable debugging mode")]
  debug: bool,
  #[clap(long, help="Enable verbose output")]
  verbose: bool,
  #[clap(subcommand)]
  command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
  #[clap(name="run", about="Start the structs daemon")]
  Run(RunOptions),
  #[clap(name="get", about="Query a value from the service")]
  Fetch(FetchOptions),
  #[clap(name="set", about="Store a value in the service")]
  Store(StoreOptions),
}

#[derive(Args, Debug)]
struct RunOptions {
  #[clap(long="socket", name="socket", help="The path to the server socket")]
  path: Option<String>,
}

#[derive(Args, Debug)]
struct FetchOptions {
  #[clap(long="socket", name="socket", help="The path to the server socket")]
  path: Option<String>,
  #[clap(help="The key to fetch the record from")]
  key: String,
}

#[derive(Args, Debug)]
struct StoreOptions {
  #[clap(long="socket", name="socket", help="The path to the server socket")]
  path: Option<String>,
  #[clap(help="The key to store the record under")]
  key: Option<String>,
}

#[derive(Clone)]
struct Socket {
	path: path::PathBuf,
}

impl Socket {
	pub fn new<P: AsRef<path::Path>>(path: P) -> Self {
		Self{
			path: path.as_ref().into(),
		}
	}

	pub fn cleanup(&mut self) -> io::Result<()> {
		match fs::remove_file(&self.path) {
			Ok(_)		 => Ok(()),
			Err(err) => {
				eprintln!("{}", &format!("* * * {}", err).yellow().bold());
				Err(err)
			},
		}
	}
}

impl Drop for Socket {
	fn drop(&mut self) {
		let _ = self.cleanup();
	}
}

fn main() {
  match cmd(){
    Ok(_)    => return,
    Err(err) => {
      eprintln!("{}", &format!("* * * {}", err).yellow().bold());
      process::exit(1);
    },
  };
}

fn cmd() -> Result<(), error::Error> {
  let opts = Options::parse();

  match &opts.command {
		Command::Run(sub)   => cmd_run(&opts, sub),
    Command::Fetch(sub) => cmd_get(&opts, sub),
    Command::Store(sub) => cmd_set(&opts, sub),
  }?;

  Ok(())
}

fn cmd_run(_opts: &Options, sub: &RunOptions) -> Result<(), error::Error> {
	let path = socket_path(&sub.path);
	let sock = Socket::new(&path);
	let path = path.as_path();
	println!("==> Listening on: {}", path.display());

	{
		let mut sock = sock.clone();
		ctrlc::set_handler(move || {
			process::exit(match sock.cleanup() {
				Ok(_)  => 0,
				Err(_) => 1,
			});
		}).expect("Could not set signal handler");
	}

	let data: BTreeMap<String, serde_json::Value> = BTreeMap::new();
	let (tx, rx) = mpsc::channel();
	thread::spawn(|| service::run(data, rx));

	let listener = UnixListener::bind(path)?;
	for stream in listener.incoming() {
		match stream {
			Ok(stream) => {
				let tx = tx.clone();
				thread::spawn(|| run_client(stream, tx));
			}
			Err(_) => {
				break;
			}
		}
	}
	Ok(())
}

fn run_client(stream: UnixStream, tx: mpsc::Sender<rpc::Request>) {
	match handle_client(stream, tx) {
		Ok(_) 	 => {},
		Err(err) => eprintln!("{}", &format!("* * * {}", err).yellow().bold()),
	};
}

fn handle_client(stream: UnixStream, tx: mpsc::Sender<rpc::Request>) -> Result<(), error::Error> {
	let mut rpc = rpc::RPC::new(stream)?;
	loop {
		let cmd = match rpc.read_cmd()? {
			Some(cmd) => cmd,
			None 			=> break,
		};
		let (rsp_tx, rsp_rx) = mpsc::channel();
		let req = rpc::Request::new(cmd, rsp_tx);
		match tx.send(req) {
			Ok(_)  => {},
			Err(_) => return Err(error::Error::SendError),
		};
		let rsp = rsp_rx.recv()?;
		println!(">>> RSP >>> {:?}", &rsp);
		rpc.write_cmd(&rsp)?;
	}
	Ok(())
}

fn cmd_get(_opts: &Options, sub: &FetchOptions) -> Result<(), error::Error> {
	let path = socket_path(&sub.path);
	if !path.exists() {
		println!(">>> NO SERVICE RUNNING (start one?)");
		return Ok(());
	}

	let stream = UnixStream::connect(path)?;
	let mut rpc = rpc::RPC::new(stream)?;

	rpc.write_cmd(&rpc::Operation::new_get(&sub.key))?;

	match rpc.expect_cmd(rpc::CMD_OK)?.data() {
		Some(data) => println!("{}", data),
		None			 => return Err(error::Error::NotFound),
	};

	Ok(())
}

fn cmd_set(_opts: &Options, sub: &StoreOptions) -> Result<(), error::Error> {
	let path = socket_path(&sub.path);
	if !path.exists() {
		println!(">>> NO SERVICE RUNNING (start one?)");
		return Ok(());
	}

	let stream = UnixStream::connect(path)?;
	let mut rpc = rpc::RPC::new(stream)?;
	let key = match &sub.key {
		Some(key) => key.to_string(),
		None 			=> Alphanumeric.sample_string(&mut rand::thread_rng(), 16),
	};

	let mut data = String::new();
	io::stdin().read_to_string(&mut data)?;
	let value: serde_json::Value = serde_json::from_str(&data)?;
	
	// re-encode the value to ensure there is no extraneous whitespace
	rpc.write_cmd(&rpc::Operation::new_set(&key, &value.to_string()))?;
	rpc.expect_cmd(rpc::CMD_OK)?;

	println!("{}", key);
	Ok(())
}

fn socket_path(path: &Option<String>) -> path::PathBuf {
	match path {
		Some(path) => path::PathBuf::from(path),
		None => {
			let mut path = env::temp_dir();
			path.push("structs.sock");
			path
		},
	}
}
