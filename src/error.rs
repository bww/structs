use std::io;
use std::fmt;
use std::str;
use std::string;
use std::sync::mpsc;

use serde_json;

#[derive(Debug)]
pub enum Error {
  IOError(io::Error),
  Utf8Error(str::Utf8Error),
  FromUtf8Error(string::FromUtf8Error),
  SerdeError(serde_json::Error),
  SendError,
  RecvError(mpsc::RecvError),
  Malformed,
  Unexpected,
  NotFound,
  ServiceError,
  Shutdown,
}

impl From<str::Utf8Error> for Error {
  fn from(err: str::Utf8Error) -> Self {
    Self::Utf8Error(err)
  }
}

impl From<string::FromUtf8Error> for Error {
  fn from(err: string::FromUtf8Error) -> Self {
    Self::FromUtf8Error(err)
  }
}

impl From<io::Error> for Error {
  fn from(err: io::Error) -> Self {
    Self::IOError(err)
  }
}

impl From<serde_json::Error> for Error {
  fn from(err: serde_json::Error) -> Self {
		Self::SerdeError(err)
  }
}

impl From<mpsc::RecvError> for Error {
  fn from(err: mpsc::RecvError) -> Self {
		Self::RecvError(err)
  }
}

impl fmt::Display for Error {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      Self::IOError(err) => err.fmt(f),
      Self::Utf8Error(err) => err.fmt(f),
      Self::FromUtf8Error(err) => err.fmt(f),
      Self::SerdeError(err) => err.fmt(f),
      Self::SendError => write!(f, "Could not send"),
      Self::RecvError(err) => err.fmt(f),
      Self::Malformed => write!(f, "Malformed"),
      Self::Unexpected => write!(f, "Unexpected"),
      Self::NotFound => write!(f, "Not found"),
      Self::Shutdown => write!(f, "Service is shutting down"),
      Self::ServiceError => write!(f, "Service error"),
    }
  }
}

