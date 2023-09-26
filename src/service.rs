use std::collections::BTreeMap;
use std::sync::mpsc;

use colored::Colorize;
use serde_json;

use crate::error;
use crate::rpc;

use crate::rpc::CMD_GET;
use crate::rpc::CMD_SET;

pub fn run(mut data: BTreeMap<String, serde_json::Value>, rx: mpsc::Receiver<rpc::Request>) -> Result<(), error::Error> {
	loop {
		let req = rx.recv()?;
		match req.name().as_ref() {
			CMD_GET => run_get(&data, req)?,
			CMD_SET => run_set(&mut data, req)?,
			cmd 		=> eprintln!("{}", &format!("* * * Unknown command: {}", cmd).yellow().bold()),
		};
		for item in &data {
			println!(">>> DATA >>> {:?}", item);
		}
	}
}

fn run_get(store: &BTreeMap<String, serde_json::Value>, mut req: rpc::Request) -> Result<(), error::Error> {
	let cmd = req.operation();
	println!(">>> GET: {:?}", cmd);
	if cmd.args().len() != 1 {
		return Err(error::Error::Malformed);
	}
	match store.get(&cmd.args()[0]) {
		Some(data) => req.send(rpc::Operation::new_found(&data.to_string()))?,
		None			 => req.send(rpc::Operation::new_none())?,
	};
	Ok(())
}

fn run_set(store: &mut BTreeMap<String, serde_json::Value>, mut req: rpc::Request) -> Result<(), error::Error> {
	let cmd = req.operation();
	println!(">>> SET: {:?}", cmd);
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
