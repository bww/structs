use std::os::unix::net::UnixStream;
use std::sync::mpsc;

use colored::Colorize;

use crate::Options;
use crate::error;
use crate::rpc;
use crate::log;

pub fn run(opts: Options, stream: UnixStream, tx: mpsc::Sender<rpc::Request>) {
  match handle(&opts, stream, tx) {
    Ok(_)    => {},
    Err(err) => log::logln!("{}", &format!("* * * {}", err).yellow().bold()),
  };
}

fn handle(opts: &Options, stream: UnixStream, tx: mpsc::Sender<rpc::Request>) -> Result<(), error::Error> {
  let mut rpc = rpc::RPC::new(stream, rpc::Options{debug: opts.debug})?;
  loop {
    let cmd = match rpc.read_cmd()? {
      Some(cmd) => cmd,
      None      => break,
    };
    let (rsp_tx, rsp_rx) = mpsc::channel();
    let req = rpc::Request::new(cmd, rsp_tx);
    match tx.send(req) {
      Ok(_)  => {},
      Err(_) => return Err(error::Error::SendError),
    };
    let rsp = rsp_rx.recv()?;
    if opts.debug {
      log::logln!("<<< {:?}", &rsp);
    }
    rpc.write_cmd(&rsp)?;
  }
  Ok(())
}
