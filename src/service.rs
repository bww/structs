use std::collections::BTreeMap;
use std::sync::mpsc;

use colored::Colorize;
use serde_json;

use crate::Options;
use crate::error;
use crate::rpc;

use crate::rpc::CMD_GET;
use crate::rpc::CMD_SET;

pub fn run(opts: Options, mut data: BTreeMap<String, serde_json::Value>, rx: mpsc::Receiver<rpc::Request>) -> Result<(), error::Error> {
	loop {
		let req = rx.recv()?;
		match req.name().as_ref() {
			CMD_GET => run_get(&opts, &data, req)?,
			CMD_SET => run_set(&opts, &mut data, req)?,
			cmd 		=> eprintln!("{}", &format!("* * * Unknown command: {}", cmd).yellow().bold()),
		};
	}
}

fn run_get(opts: &Options, store: &BTreeMap<String, serde_json::Value>, mut req: rpc::Request) -> Result<(), error::Error> {
	let cmd = req.operation();
	if opts.debug {
		println!(">>> {:?}", cmd);
	}
	if cmd.args().len() != 1 {
		return Err(error::Error::Malformed);
	}
	let name = cmd.args()[0].to_owned();
	match store.get(&name) {
		Some(data) => req.send(rpc::Operation::new_found(&name, &data.to_string()))?,
		None			 => req.send(rpc::Operation::new_none(&name))?,
	};
	Ok(())
}

fn run_set(opts: &Options, store: &mut BTreeMap<String, serde_json::Value>, mut req: rpc::Request) -> Result<(), error::Error> {
	let cmd = req.operation();
	if opts.debug {
		println!(">>> {:?}", cmd);
	}
	if cmd.args().len() != 1 {
		return Err(error::Error::Malformed);
	}
	let data = match cmd.data() {
		Some(data) => serde_json::from_str(&data)?,
		None 			 => serde_json::Value::Null,
	};
	store.insert(cmd.args()[0].clone(), data);
	req.send(rpc::Operation::new_ok())?;
	Ok(())
}
