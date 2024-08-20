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
use crate::rpc::CMD_RANGE;
use crate::rpc::CMD_SET;
use crate::rpc::CMD_DELETE;
use crate::rpc::CMD_SHUTDOWN;

fn cleanup_on_signal(opts: Options, mut sock: rpc::Socket) {
  ctrlc::set_handler(move || {
    if opts.debug || opts.verbose {
      eprintln!(">>> Shutting down due to signal...");
    }
    process::exit(match sock.cleanup() {
      Ok(_)  => 0,
      Err(_) => 1,
    });
  }).expect("Could not set signal handler");
}

fn cleanup_on_idle(opts: Options, mut sock: rpc::Socket, dur: time::Duration) -> Result<mpsc::Sender<()>, error::Error> {
  if opts.debug {
    eprintln!(">>> Idle timeout: {:?}", &dur);
  }
  let mut last_op = time::SystemTime::now();
  let (tx, rx) = mpsc::channel();
  thread::spawn(move || {
    loop {
      if let Err(err) = rx.recv_timeout(dur) {
        match err {
          mpsc::RecvTimeoutError::Timeout => break,  // timeout exceeded, clean up
          _                               => return, // channel ended, just return
        }
      }
      last_op = time::SystemTime::now();
    }
    if opts.debug || opts.verbose {
      eprintln!(">>> Shutting down after {:?} of inactivity...", last_op.elapsed().unwrap());
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
  let poll_tx = match runopts.timeout {
    Some(dur) => Some(cleanup_on_idle(opts.clone(), sock.clone(), dur.duration())?),
    None => None,
  };

  loop {
    let req = rx.recv()?;
    match req.name().as_ref() {
      CMD_GET      => run_get(&opts, &data, req)?,
      CMD_RANGE    => run_range(&opts, &data, req)?,
      CMD_SET      => run_set(&opts, &mut data, req)?,
      CMD_DELETE   => {
        run_delete(&opts, &mut data, req)?;
        if runopts.finalize && data.len() == 0 { break; }
      },
      CMD_SHUTDOWN => {
        run_stop(&opts, req)?;
        break;
      },
      cmd => eprintln!("{}", &format!("* * * Unknown command: {}", cmd).yellow().bold()),
    };
    if let Some(poll_tx) = &poll_tx {
      if let Err(err) = poll_tx.send(()) {
        eprintln!("{}", &format!("* * * Could not poll: {}", err).yellow().bold());
      }
    }
  }

  if opts.debug || opts.verbose {
    eprintln!(">>> Shutting down due to finalization...");
  }
  process::exit(match sock.cleanup() {
    Ok(_)  => 0,
    Err(_) => 1,
  });
}

fn fetch<'a>(store: &'a BTreeMap<String, serde_json::Value>, key: &str) -> Result<&'a serde_json::Value, error::Error> {
  let path = jsonpath::Path::new(key);
  let (key, path) = path.next();
  let key = match key {
    Some(key) => key,
    None => return Err(error::Error::Malformed),
  };
  let (data, rest) = if let Some(data) = store.get(key) {
    match path {
      Some(path) => jsonpath::Path::new(path).find(data),
      None       => (Some(data), None),
    }
  } else {
    (None, None)
  };
  match rest {
    Some(_) => Err(error::Error::NotFound),
    None    => match data {
      Some(data) => Ok(data),
      None       => Err(error::Error::NotFound),
    }
  }
}

fn store<'a>(store: &'a mut BTreeMap<String, serde_json::Value>, key: &str, val: serde_json::Value) -> Result<serde_json::Value, error::Error> {
  let path = jsonpath::Path::new(key);
  // split into the key and path
  let (key, path) = match path.next() {
    (Some(key), Some(path)) => (key, jsonpath::Path::new(path)),
    _                       => return Err(error::Error::Malformed),
  };
  // split into the path to the leaf node we're referencing and the leaf identifier
  let (path, leaf) = match path.trim(1) {
    (Some(p), Some(l)) => (p, Some(l)),
    (Some(p), None)    => (p, None),
    _                  => return Err(error::Error::Malformed),
  };
  // this is the structure we're referencing into; we create a copy which we'll mutate
  let data = match store.get(key) {
    Some(data) => data.clone(),
    None       => return Err(error::Error::NotFound),
  };
  // this is the value at the specific path we're referencing
  let rval = path.value(&data);
  let rval = match leaf {
    Some(leaf) => {
      let mut base = match rval {
        Some(rval) => rval.clone(),
        None       => return Err(error::Error::NotFound),
      };
      match &mut base {
        serde_json::Value::Object(v) => {
          v.insert(leaf.to_string(), val);
        },
        serde_json::Value::Array(v) => match jsonpath::index(&base, leaf) {
          Some(i) => v[i] = val,
          None    => return Err(error::Error::Malformed),
        },
        _ => return Err(error::Error::Malformed), // other types cannot be updated
      }
      base
    },
    None => return Err(error::Error::NotFound),
  };
  // persist a copy in the store, return the updated value
  store.insert(key.to_owned(), rval.clone());
  Ok(rval)
}

fn run_get(opts: &Options, store: &BTreeMap<String, serde_json::Value>, mut req: rpc::Request) -> Result<(), error::Error> {
  let cmd = req.operation();
  if opts.debug {
    eprintln!(">>> {:?}", cmd);
  }
  if cmd.args().len() != 1 {
    return Err(error::Error::Malformed);
  }
  let name = cmd.args()[0].to_string();
  match fetch(store, &name) {
    Ok(data) => req.send(rpc::Operation::new_found(&name, &data.to_string()))?,
    Err(err) => match err {
      error::Error::NotFound => req.send(rpc::Operation::new_none(&name))?,
      _                      => return Err(err),
    },
  }
  Ok(())
}

fn run_range(opts: &Options, store: &BTreeMap<String, serde_json::Value>, mut req: rpc::Request) -> Result<(), error::Error> {
  let cmd = req.operation();
  if opts.debug {
    eprintln!(">>> {:?}", cmd);
  }
  if cmd.args().len() != 1 {
    return Err(error::Error::Malformed);
  }
  let name = cmd.args()[0].to_string();
  let data = match fetch(store, &name) {
    Ok(data) => data,
    Err(err) => match err {
      error::Error::NotFound => return req.send(rpc::Operation::new_none(&name)),
      _                      => return Err(err),
    },
  };
  let range = match data {
    serde_json::Value::Array(v)  => Some((0..v.len()).map(|e| { serde_json::Value::Number(e.into()) }).collect::<Vec<serde_json::Value>>()),
    serde_json::Value::Object(v) => Some(v.keys().map(|e| { serde_json::Value::String(e.to_string()) }).collect::<Vec<serde_json::Value>>()),
    _                            => None,
  };
  match range {
    Some(range) => req.send(rpc::Operation::new_found(&name, &serde_json::Value::Array(range).to_string()))?,
    None        => req.send(rpc::Operation::new_none(&name))?,
  }
  Ok(())
}

fn run_set(opts: &Options, store: &mut BTreeMap<String, serde_json::Value>, mut req: rpc::Request) -> Result<(), error::Error> {
  let cmd = req.operation();
  if opts.debug {
    eprintln!(">>> {:?}", cmd);
  }
  if cmd.args().len() != 1 {
    return Err(error::Error::Malformed);
  }
  let data = match cmd.data() {
    Some(data) => serde_json::from_str(&data)?,
    None       => serde_json::Value::Null,
  };
  store.insert(cmd.args()[0].clone(), data);
  req.send(rpc::Operation::new_ok())?;
  Ok(())
}

fn run_delete(opts: &Options, store: &mut BTreeMap<String, serde_json::Value>, mut req: rpc::Request) -> Result<(), error::Error> {
  let cmd = req.operation();
  if opts.debug {
    eprintln!(">>> {:?}", cmd);
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
    eprintln!(">>> {:?}", cmd);
  }
  req.send(rpc::Operation::new_ok())?;
  Ok(())
}
