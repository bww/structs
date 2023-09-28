use std::collections::BTreeMap;
use std::sync::mpsc;
use std::process;

use colored::Colorize;
use serde_json;
use ctrlc;

use crate::Options;
use crate::RunOptions;
use crate::error;
use crate::rpc;
use crate::jsonpath;

use crate::rpc::CMD_GET;
use crate::rpc::CMD_SET;
use crate::rpc::CMD_DELETE;
use crate::rpc::CMD_SHUTDOWN;

pub fn run(opts: Options, runopts: RunOptions, mut data: BTreeMap<String, serde_json::Value>, mut sock: rpc::Socket, rx: mpsc::Receiver<rpc::Request>) -> Result<(), error::Error> {
	cleanup_on_signal(sock.clone());

	loop {
		let req = rx.recv()?;
		match req.name().as_ref() {
			CMD_GET 		 => run_get(&opts, &data, req)?,
			CMD_SET 		 => run_set(&opts, &mut data, req)?,
			CMD_DELETE	 => {
				run_delete(&opts, &mut data, req)?;
				if runopts.finalize && data.len() == 0 { break; }
			},
			CMD_SHUTDOWN => {
				run_stop(&opts, req)?;
				break;
			},
			cmd => eprintln!("{}", &format!("* * * Unknown command: {}", cmd).yellow().bold()),
		};
	}

	process::exit(match sock.cleanup() {
		Ok(_)  => 0,
		Err(_) => 1,
	});
}

fn cleanup_on_signal(mut sock: rpc::Socket) {
	ctrlc::set_handler(move || {
		process::exit(match sock.cleanup() {
			Ok(_)  => 0,
			Err(_) => 1,
		});
	}).expect("Could not set signal handler");
}

fn run_get(opts: &Options, store: &BTreeMap<String, serde_json::Value>, mut req: rpc::Request) -> Result<(), error::Error> {
	let cmd = req.operation();
	if opts.debug {
		println!(">>> {:?}", cmd);
	}
	if cmd.args().len() != 1 {
		return Err(error::Error::Malformed);
	}
	let jp = jsonpath::Path::new(&cmd.args()[0]);
	let (name, path) = jp.next();
	let name = match name {
		Some(name) => name,
		None => return Err(error::Error::Malformed),
	};
	let (data, rest) = if let Some(data) = store.get(name) {
		match path {
			Some(path) => jsonpath::Path::new(path).find(data),
			None 			 => (Some(data), None),
		}
	} else {
		(None, None)
	};
	match rest {
		Some(_) => req.send(rpc::Operation::new_none(name))?,
		None 		=> match data { 
			Some(data) => req.send(rpc::Operation::new_found(name, &data.to_string()))?,
			None			 => req.send(rpc::Operation::new_none(name))?,
		}
	}
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

fn run_delete(opts: &Options, store: &mut BTreeMap<String, serde_json::Value>, mut req: rpc::Request) -> Result<(), error::Error> {
	let cmd = req.operation();
	if opts.debug {
		println!(">>> {:?}", cmd);
	}
	if cmd.args().len() != 1 {
		return Err(error::Error::Malformed);
	}
	store.remove(&cmd.args()[0]);
	req.send(rpc::Operation::new_ok())?;
	Ok(())
}

fn run_stop(opts: &Options,  mut req: rpc::Request) -> Result<(), error::Error> {
	let cmd = req.operation();
	if opts.debug {
		println!(">>> {:?}", cmd);
	}
	req.send(rpc::Operation::new_ok())?;
	Ok(())
}

