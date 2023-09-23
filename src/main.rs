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
use std::sync::mpsc::channel;
use ctrlc;

use colored::Colorize;
use clap::{Parser, Subcommand, Args};

use serde::{Serialize, Deserialize};
use serde_json;

mod error;

const VERSION: &str = env!("CARGO_PKG_VERSION");

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

	pub fn read_cmd(&mut self) -> Result<Option<String>, error::Error> {
		let mut line = String::new();
		match self.reader.read_line(&mut line)? {
			0 => Ok(None),
			_ => Ok(Some(line.trim().to_string())),
		}
	}

	pub fn write_cmd(&mut self, cmd: &str) -> Result<(), error::Error> {
		self.writer.write_all(cmd.as_bytes())?;
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
		println!("==> Cleaning up: {}", self.path.display());
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

	let listener = UnixListener::bind(path)?;
	for stream in listener.incoming() {
		match stream {
			Ok(stream) => {
				thread::spawn(|| run_client(stream));
			}
			Err(err) => {
				break;
			}
		}
	}
	Ok(())
}

fn run_client(mut stream: UnixStream) {
	println!("Start");
	match handle_client(stream) {
		Ok(_) 	 => {},
		Err(err) => eprintln!("{}", &format!("* * * {}", err).yellow().bold()),
	};
	println!("Client ended.");
}

fn handle_client(mut stream: UnixStream) -> Result<(), error::Error> {
	let mut rpc = RPC::new(stream)?;
	loop {
		match rpc.read_cmd()? {
			Some(cmd) => match cmd.as_ref() {
				"hi" => rpc.write_cmd("Hello world")?,
				cmd	 => println!(">>> UNKNOWN: {}", cmd),
			},
			None => break,
		};
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

	rpc.write_cmd("hi")?;
	match rpc.read_cmd()? {
		Some(cmd) => println!(">>> {}", cmd),
		None 			=> println!(">>> <none>"),
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
	let mut buf = String::new();
	stream.read_to_string(&mut buf)?;
	println!(">>> {}", buf);

	let mut data = String::new();
	io::stdin().read_to_string(&mut data)?;
	let data: serde_json::Value = serde_json::from_str(&data)?;
	println!(">>> STORE: {}", data);
	
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
