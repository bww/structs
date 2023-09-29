use std::io;
use std::io::prelude::*;
use std::fs;
use std::path;

use std::os::unix::net::UnixStream;
use std::sync::mpsc;

use crate::error;

pub const CMD_SET: 	 		 &str = "set";
pub const CMD_GET: 	 		 &str = "get";
pub const CMD_RANGE: 	 	 &str = "range";
pub const CMD_FOUND: 		 &str = "found";
pub const CMD_NONE:  		 &str = "none";
pub const CMD_DELETE:  	 &str = "delete";
pub const CMD_SHUTDOWN:  &str = "stop";
pub const CMD_OK:  	 		 &str = "ok";

#[derive(Debug)]
pub struct Operation {
	name: String,
	args: Vec<String>,
	data: Option<String>,
}

impl Operation {
	pub fn new(name: &str, args: &[&str], data: Option<&str>) -> Self {
		Operation{
			name: name.to_owned(),
			args: args.iter().map(|e| { e.to_string() }).collect(),
			data: match data {
				Some(data) => Some(data.to_string()),
				None => None,
			},
		}
	}

	pub fn name<'a>(&'a self) -> &'a str {
		&self.name
	}

	pub fn data<'a>(&'a self) -> &'a Option<String> {
		&self.data
	}

	pub fn args<'a>(&'a self) -> &'a [String] {
		&self.args
	}

	pub fn new_ok() -> Self {
		Self::new(CMD_OK, &[], None)
	}

	pub fn new_found(name: &str, data: &str) -> Self {
		Self::new(CMD_FOUND, &[name], Some(data))
	}

	pub fn new_none(name: &str) -> Self {
		Self::new(CMD_NONE, &[name], None)
	}

	pub fn new_get(name: &str) -> Self {
		Self::new(CMD_GET, &[name], None)
	}

	pub fn new_range(name: &str) -> Self {
		Self::new(CMD_RANGE, &[name], None)
	}

	pub fn new_set(name: &str, data: &str) -> Self {
		Self::new(CMD_SET, &[name], Some(data))
	}

	pub fn new_delete(name: &str) -> Self {
		Self::new(CMD_DELETE, &[name], None)
	}

	pub fn new_shutdown() -> Self {
		Self::new(CMD_SHUTDOWN, &[], None)
	}
}

pub struct Request {
	op: Operation,
	tx: mpsc::Sender<Operation>,
}

impl Request {
	pub fn new(op: Operation, tx: mpsc::Sender<Operation>) -> Self {
		Request{
			op: op,
			tx: tx,
		}
	}

	pub fn name<'a>(&'a self) -> &'a str {
		self.op.name()
	}

	pub fn operation<'a>(&'a mut self) -> &'a Operation {
		&self.op
	}

	pub fn send(&self, op: Operation) -> Result<(), error::Error> {
		match self.tx.send(op) {
			Ok(_)  => Ok(()),
			Err(_) => Err(error::Error::SendError),
		}
	}
}

pub struct RPC {
	reader: io::BufReader<UnixStream>,
	writer: UnixStream,
}

impl RPC {
	pub fn new(stream: UnixStream) -> Result<Self, error::Error> {
		let writer = stream.try_clone()?;
		let reader = io::BufReader::new(stream);
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
		let data = if match args[0] {
			CMD_SET | CMD_FOUND => true,
			_ 									=> false,
		}{
			match self.reader.read_line(&mut line)? {
				0 => return Err(error::Error::Malformed),
				_ => Some(line.trim().to_string()),
			}
		} else {
			None
		};

		Ok(Some(Operation::new(args[0], &args[1..], data.as_deref())))
	}

	pub fn expect_cmd(&mut self, expect: &[&str]) -> Result<Operation, error::Error> {
		match self.read_cmd()? {
			Some(cmd) => if expect.iter().any(|&e| { cmd.name == e}) {
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
pub struct Socket {
	path: path::PathBuf,
}

impl Socket {
	pub fn new<P: AsRef<path::Path>>(path: P) -> Self {
		Self{
			path: path.as_ref().into(),
		}
	}

	pub fn cleanup(&mut self) -> io::Result<()> {
		fs::remove_file(&self.path)
	}
}

impl Drop for Socket {
	fn drop(&mut self) {
		self.cleanup().expect("Could not remove socket");
	}
}

