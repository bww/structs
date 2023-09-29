use std::time;
use std::thread;
use std::process;
use std::collections::BTreeMap;
use std::sync::mpsc;

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

fn cleanup_on_signal(opts: Options, mut sock: rpc::Socket) {
	ctrlc::set_handler(move || {
		if opts.debug || opts.verbose {
			println!(">>> Shutting down due to signal...");
		}
		process::exit(match sock.cleanup() {
			Ok(_)  => 0,
			Err(_) => 1,
		});
	}).expect("Could not set signal handler");
}

fn cleanup_on_idle(opts: Options, mut sock: rpc::Socket, dur: time::Duration) -> Result<mpsc::Sender<time::Duration>, error::Error> {
	let (tx, rx) = mpsc::channel();
	thread::spawn(move || {
		loop {
			println!(">>> HOW WE DOIN?");
			match rx.recv_timeout(dur) {
				Ok(when) => println!(">>> POLL: {:?}", when),
				Err(err) => match err {
					mpsc::RecvTimeoutError::Timeout => break,  // timeout exceeded, clean up
					_          										  => return, // channel ended, just return
				},
			}
		}
		if opts.debug || opts.verbose {
			println!(">>> Shutting down due to idle timeout...");
		}
		process::exit(match sock.cleanup() {
			Ok(_)  => 0,
			Err(_) => 1,
		});
	});
	Ok(tx)
}

pub fn run(opts: Options, runopts: RunOptions, mut data: BTreeMap<String, serde_json::Value>, mut sock: rpc::Socket, rx: mpsc::Receiver<rpc::Request>) -> Result<(), error::Error> {
	cleanup_on_signal(opts.clone(), sock.clone());
	let poll_tx = cleanup_on_idle(opts.clone(), sock.clone(), time::Duration::from_secs(10))?;
	let mut last_op = time::SystemTime::now();

	loop {
		let req = rx.recv()?;
		if opts.debug && opts.verbose {
			let now = time::SystemTime::now();
			println!(">>> {:?} since last operation", last_op.elapsed()?);
			last_op = now;
		}
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
		if let Err(err) = poll_tx.send(last_op.elapsed()?) {
			eprintln!("{}", &format!("* * * Could not poll: {}", err).yellow().bold());
		}
	}

	if opts.debug || opts.verbose {
		println!(">>> Shutting down due to finalization...");
	}
	process::exit(match sock.cleanup() {
		Ok(_)  => 0,
		Err(_) => 1,
	});
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

