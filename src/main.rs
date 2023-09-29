use std::io;
use std::env;
use std::str;
use std::path;
use std::time;
use std::process;
use std::io::Read;

use std::thread;
use std::os::unix::net::{UnixStream, UnixListener};
use std::sync::mpsc;

use std::collections::BTreeMap;
use rand::distributions::{Alphanumeric, DistString};

use colored::Colorize;
use clap::{Parser, Subcommand, Args};

use serde_json;

mod error;
mod rpc;
mod service;
mod client;
mod duration;
mod jsonpath;

const _VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Parser, Debug, Clone)]
#[clap(author, version, about, long_about = None)]
pub struct Options {
  #[clap(long, help="Enable debugging mode")]
  pub debug: bool,
  #[clap(long, help="Enable verbose output")]
  pub verbose: bool,
  #[clap(subcommand)]
  command: Command,
}

#[derive(Subcommand, Debug, Clone)]
enum Command {
  #[clap(name="run", about="Start the structs daemon")]
  Run(RunOptions),
  #[clap(name="get", about="Query a value from the service")]
  Fetch(FetchOptions),
  #[clap(name="set", about="Store a value in the service")]
  Store(StoreOptions),
  #[clap(name="rm", about="Delete a value from the service")]
  Delete(DeleteOptions),
  #[clap(name="stop", about="Shutdown the service, if it is running")]
  Shutdown(ShutdownOptions),
}

#[derive(Args, Debug, Clone)]
pub struct RunOptions {
  #[clap(long="timeout", default_value="1m", help="Shut down the service after the last entry is deleted")]
  pub timeout: duration::Duration,
  #[clap(long="finalize", help="Shut down the service after the last entry is deleted")]
  pub finalize: bool,
  #[clap(long="socket", name="socket", help="The path to the server socket")]
  path: Option<String>,
}

#[derive(Args, Debug, Clone)]
struct FetchOptions {
  #[clap(long="socket", name="socket", help="The path to the server socket")]
  path: Option<String>,
  #[clap(help="The key to fetch the record from")]
  key: String,
}

#[derive(Args, Debug, Clone)]
struct StoreOptions {
  #[clap(long="socket", name="socket", help="The path to the server socket")]
  path: Option<String>,
  #[clap(help="The key to store the record under")]
  key: Option<String>,
}

#[derive(Args, Debug, Clone)]
struct DeleteOptions {
  #[clap(long="socket", name="socket", help="The path to the server socket")]
  path: Option<String>,
  #[clap(help="The key to the record to delete")]
  key: String,
}

#[derive(Args, Debug, Clone)]
struct ShutdownOptions {
  #[clap(long="socket", name="socket", help="The path to the server socket")]
  path: Option<String>,
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

fn run_svc<P: AsRef<path::Path>>(opts: &Options, path: P) -> Result<(), error::Error> {
	let me = env::current_exe()?;
	if opts.debug {
		eprintln!(">>> No service running; starting: {}", me.display());
	}
	process::Command::new(me).arg("run").arg("--finalize").spawn()?;
	let mut dur = time::Duration::from_millis(1);
	for _ in 0..5 {
		thread::sleep(dur);
		if path.as_ref().exists() {
			return Ok(());
		}
		dur *= 10 // backoff
	}
	Err(error::Error::ServiceError)
}

fn cmd() -> Result<(), error::Error> {
  let opts = Options::parse();

  match &opts.command {
		Command::Run(sub)			 => cmd_run(&opts, sub),
    Command::Fetch(sub)		 => cmd_get(&opts, sub),
    Command::Store(sub)		 => cmd_set(&opts, sub),
    Command::Delete(sub)	 => cmd_delete(&opts, sub),
    Command::Shutdown(sub) => cmd_stop(&opts, sub),
  }?;

  Ok(())
}

fn cmd_run(opts: &Options, sub: &RunOptions) -> Result<(), error::Error> {
	let path = socket_path(&sub.path);
	let sock = rpc::Socket::new(&path);
	let path = path.as_path();
	println!("==> Listening on: {}", path.display());

	let data: BTreeMap<String, serde_json::Value> = BTreeMap::new();
	let (tx, rx) = mpsc::channel();
	let svcopts = opts.clone();
	let runopts = sub.clone();
	thread::spawn(|| service::run(svcopts, runopts, data, sock, rx));

	let listener = UnixListener::bind(path)?;
	for stream in listener.incoming() {
		match stream {
			Ok(stream) => {
				let tx = tx.clone();
				let cliopts = opts.clone();
				thread::spawn(|| client::run(cliopts, stream, tx));
			}
			Err(_) => {
				break;
			}
		}
	}
	Ok(())
}

fn cmd_stop(_opts: &Options, sub: &ShutdownOptions) -> Result<(), error::Error> {
	let path = socket_path(&sub.path);
	if !path.exists() {
		return Ok(()); // no service running, nothing to stop
	}

	let stream = UnixStream::connect(path)?;
	let mut rpc = rpc::RPC::new(stream)?;

	rpc.write_cmd(&rpc::Operation::new_shutdown())?;
	rpc.expect_cmd(&[rpc::CMD_OK])?;

	Ok(())
}

fn cmd_get(opts: &Options, sub: &FetchOptions) -> Result<(), error::Error> {
	let path = socket_path(&sub.path);
	if !path.exists() {
		run_svc(opts, &path)?;
	}

	let stream = UnixStream::connect(path)?;
	let mut rpc = rpc::RPC::new(stream)?;

	rpc.write_cmd(&rpc::Operation::new_get(&sub.key))?;

	let rsp = rpc.expect_cmd(&[rpc::CMD_FOUND, rpc::CMD_NONE])?;
	let data = match rsp.name() {
		rpc::CMD_NONE  => Err(error::Error::NotFound),
		rpc::CMD_FOUND => match rsp.data() {
			Some(data) => Ok(data),
			None			 => Err(error::Error::Malformed),
		},
		_ => Err(error::Error::Malformed),
	}?;

	println!("{}", data);
	Ok(())
}

fn cmd_set(opts: &Options, sub: &StoreOptions) -> Result<(), error::Error> {
	let path = socket_path(&sub.path);
	if !path.exists() {
		run_svc(opts, &path)?;
	}

	let stream = UnixStream::connect(path)?;
	let mut rpc = rpc::RPC::new(stream)?;
	let key = match &sub.key {
		Some(key) => key.to_string(),
		None			=> Alphanumeric.sample_string(&mut rand::thread_rng(), 16),
	};

	let mut data = String::new();
	io::stdin().read_to_string(&mut data)?;
	let value: serde_json::Value = serde_json::from_str(&data)?;
	
	// re-encode the value to ensure there is no extraneous whitespace
	rpc.write_cmd(&rpc::Operation::new_set(&key, &value.to_string()))?;
	rpc.expect_cmd(&[rpc::CMD_OK])?;

	println!("{}", key);
	Ok(())
}

fn cmd_delete(_opts: &Options, sub: &DeleteOptions) -> Result<(), error::Error> {
	let path = socket_path(&sub.path);
	if !path.exists() {
		return Ok(()); // no service running, nothing do delete
	}

	let stream = UnixStream::connect(path)?;
	let mut rpc = rpc::RPC::new(stream)?;

	rpc.write_cmd(&rpc::Operation::new_delete(&sub.key))?;
	rpc.expect_cmd(&[rpc::CMD_OK])?;

	println!("{}", &sub.key);
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
