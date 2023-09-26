use std::io;
use std::io::prelude::*;

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

const VERSION: &str = env!("CARGO_PKG_VERSION");

const CMD_SET: &str = "set";
const CMD_GET: &str = "get";
const CMD_OK:  &str = "ok";

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

#[derive(Debug)]
pub struct Operation {
	name: String,
	args: Vec<String>,
	data: Option<String>,
}

impl Operation {
	fn new(name: &str, args: &[&str], data: Option<&str>) -> Self {
		Operation{
			name: name.to_owned(),
			args: args.iter().map(|e| { e.to_string() }).collect(),
			data: match data {
				Some(data) => Some(data.to_string()),
				None => None,
			},
		}
	}

	fn new_ok() -> Self {
		Self::new(CMD_OK, &[], None)
	}

	fn new_none() -> Self {
		Self::new(CMD_OK, &[], None)
	}

	fn new_found(data: &str) -> Self {
		Self::new(CMD_OK, &[], Some(data))
	}

	fn new_get(name: &str) -> Self {
		Self::new(CMD_GET, &[name], None)
	}

	fn new_set(name: &str, data: &str) -> Self {
		Self::new(CMD_SET, &[name], Some(data))
	}
}

pub struct OperationRequest {
	op: Operation,
	tx: mpsc::Sender<Operation>,
}

impl OperationRequest {
	fn new(op: Operation, tx: mpsc::Sender<Operation>) -> Self {
		OperationRequest{
			op: op,
			tx: tx,
		}
	}

	pub fn name<'a>(&'a self) -> &'a str {
		&self.op.name
	}

	pub fn operation<'a>(&'a mut self) -> &'a Operation {
		&self.op
	}

	pub fn send(&self, op: Operation) -> Result<(), error::Error> {
		match self.tx.send(op) {
			Ok(_) 	 => Ok(()),
			Err(err) => Err(error::Error::SendError),
		}
	}
}

struct RPC {
	reader: io::BufReader<UnixStream>,
	writer: UnixStream,
}

impl RPC {
	pub fn new(mut stream: UnixStream) -> Result<Self, error::Error> {
		let mut writer = stream.try_clone()?;
		let mut reader = io::BufReader::new(stream);
		Ok(Self{
			reader: reader,
			writer: writer,
		})
	}

	pub fn read_cmd(&mut self) -> Result<Option<Operation>, error::Error> {
		let mut line = String::new();
		let res = match self.reader.read_line(&mut line)? {
			0 => return Ok(None),
			_ => line.trim(),
		};

		let mut text = res;
		let mut args: Vec<&str> = Vec::new();
		loop {
			match text.split_once(" ") {
				Some((l, r)) => {
					args.push(l);
					text = r;
				},
				None => {
					args.push(text);
					break;
				},
			}
		}

		if args.len() < 1 {
			return Err(error::Error::Malformed);
		}

		let mut line = String::new();
		let data = match args[0] {
			CMD_SET => match self.reader.read_line(&mut line)? {
				0 => return Err(error::Error::Malformed),
				_ => Some(line.trim().to_string()),
			},
			_ => None,
		};

		Ok(Some(Operation::new(args[0], &args[1..], data.as_deref())))
	}

	pub fn expect_cmd(&mut self, expect: &str) -> Result<Operation, error::Error> {
		match self.read_cmd()? {
			Some(cmd) => if cmd.name == expect {
				Ok(cmd)
			}else{
				Err(error::Error::Unexpected)
			},
			None => Err(error::Error::Unexpected),
		}
	}

	pub fn write_cmd(&mut self, cmd: &Operation) -> Result<(), error::Error> {
		let mut line: Vec<&str> = Vec::new();
		line.push(&cmd.name);
		for arg in &cmd.args {
			line.push(arg);
		}
		self.write_line(&line)?;
		if let Some(data) = &cmd.data {
			self.write_line(&[&data])?;
		}
		Ok(())
	}

	pub fn write_line(&mut self, line: &[&str]) -> Result<(), error::Error> {
		let mut i = 0;
		for cmd in line { 
			if i > 0 { self.writer.write_all(b" ")?; }
			self.writer.write_all(cmd.trim().as_bytes())?;
			i += 1;
		}
		self.writer.write_all(b"\n")?;
		self.writer.flush()?;
		Ok(())
	}
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

fn cmd_run(opts: &Options, sub: &RunOptions) -> Result<(), error::Error> {
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
			Err(err) => {
				break;
			}
		}
	}
	Ok(())
}

fn run_client(mut stream: UnixStream, tx: mpsc::Sender<OperationRequest>) {
	match handle_client(stream, tx) {
		Ok(_) 	 => {},
		Err(err) => eprintln!("{}", &format!("* * * {}", err).yellow().bold()),
	};
}

fn handle_client(mut stream: UnixStream, tx: mpsc::Sender<OperationRequest>) -> Result<(), error::Error> {
	let mut rpc = RPC::new(stream)?;
	loop {
		let cmd = match rpc.read_cmd()? {
			Some(cmd) => cmd,
			None 			=> break,
		};
		match cmd.name.as_ref() {
			CMD_GET => rpc.write_cmd(&Operation::new_found("THE DATA"))?,
			CMD_SET => rpc.write_cmd(&Operation::new_ok())?,
			cmd 		=> {
				eprintln!("{}", &format!("* * * Unknown command: {}", cmd).yellow().bold());
				break;
			},
		};
		let (rsp_tx, rsp_rx) = mpsc::channel();
		let mut req = OperationRequest::new(cmd, rsp_tx);
		match tx.send(req) {
			Ok(_) 	 => {},
			Err(err) => return Err(error::Error::SendError),
		};
		println!(">>> !!! >>> {:?}", rsp_rx.recv());
	}
	Ok(())
}

fn cmd_get(opts: &Options, sub: &FetchOptions) -> Result<(), error::Error> {
	let path = socket_path(&sub.path);
	if !path.exists() {
		println!(">>> NO SERVICE RUNNING (start one?)");
		return Ok(());
	}

	let mut stream = UnixStream::connect(path)?;
	let mut rpc = RPC::new(stream)?;

	rpc.write_cmd(&Operation::new_get(&sub.key))?;

	match rpc.expect_cmd(CMD_OK)?.data {
		Some(data) => println!("{}", data),
		None			 => return Err(error::Error::NotFound),
	};

	Ok(())
}

fn cmd_set(opts: &Options, sub: &StoreOptions) -> Result<(), error::Error> {
	let path = socket_path(&sub.path);
	if !path.exists() {
		println!(">>> NO SERVICE RUNNING (start one?)");
		return Ok(());
	}

	let mut stream = UnixStream::connect(path)?;
	let mut rpc = RPC::new(stream)?;
	let key = match &sub.key {
		Some(key) => key.to_string(),
		None 			=> Alphanumeric.sample_string(&mut rand::thread_rng(), 16),
	};

	let mut data = String::new();
	io::stdin().read_to_string(&mut data)?;
	let value: serde_json::Value = serde_json::from_str(&data)?;
	
	// re-encode the value to ensure there is no extraneous whitespace
	rpc.write_cmd(&Operation::new_set(&key, &value.to_string()))?;
	rpc.expect_cmd(CMD_OK)?;

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
