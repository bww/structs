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
  #[clap(help="The key to store the record under")]
  key: String,
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
	println!("Gotcha");
	match handle_client(stream) {
		Ok(_) 	 => {},
		Err(err) => eprintln!("{}", &format!("* * * {}", err).yellow().bold()),
	};
}

fn handle_client(mut stream: UnixStream) -> Result<(), error::Error> {
	stream.write_all(b"Hello world.\n")?;
	Ok(())
}

fn cmd_get(opts: &Options, sub: &FetchOptions) -> Result<(), error::Error> {
	let path = socket_path(&sub.path);
	let mut stream = UnixStream::connect(path)?;
	let mut buf = String::new();
	stream.read_to_string(&mut buf)?;
	println!(">>> {}", buf);
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
