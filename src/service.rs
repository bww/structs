use std::collections::BTreeMap;
use std::sync::mpsc;

use colored::Colorize;
use serde_json;

use crate::error;
use crate::Operation;

use crate::CMD_GET;
use crate::CMD_SET;

pub fn run(mut data: BTreeMap<String, serde_json::Value>, rx: mpsc::Receiver<Operation>) -> Result<(), error::Error> {
	loop {
		let cmd = rx.recv()?;
		match cmd.name.as_ref() {
			CMD_GET => run_get(&data, cmd)?,
			CMD_SET => run_set(&mut data, cmd)?,
			cmd 		=> eprintln!("{}", &format!("* * * Unknown command: {}", cmd).yellow().bold()),
		};
	}
}

fn run_get(store: &BTreeMap<String, serde_json::Value>, cmd: Operation) -> Result<(), error::Error> {
	if cmd.args.len() != 1 {
		return Err(error::Error::Malformed);
	}
	match store.get(&cmd.args[0]) {
		Some(data) => println!(">>> OK: {}", data),
		None			 => println!(">>> OK: <NONE>"),
	};
	Ok(())
}

fn run_set(store: &mut BTreeMap<String, serde_json::Value>, cmd: Operation) -> Result<(), error::Error> {
	if cmd.args.len() != 1 {
		return Err(error::Error::Malformed);
	}
	let data = match cmd.data {
		Some(data) => serde_json::from_str(&data)?,
		None 			 => serde_json::Value::Null,
	};
	store.insert(cmd.args[0].to_owned(), data);
	Ok(())
}
