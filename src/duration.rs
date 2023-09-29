use std::fmt;
use std::num;
use std::str::FromStr;
use std::time;

use nom;

#[derive(Debug, PartialEq)]
pub enum Error {
	ParseIntError(num::ParseIntError),
	ParseSyntaxError,
	ParseDurationError,
}

impl From<num::ParseIntError> for Error {
  fn from(err: num::ParseIntError) -> Self {
    Self::ParseIntError(err)
  }
}

impl<E> From<nom::Err<E>> for Error {
  fn from(_: nom::Err<E>) -> Self {
    Self::ParseSyntaxError
  }
}

impl std::error::Error for Error {}

impl fmt::Display for Error {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
    	Self::ParseIntError(err) => err.fmt(f),
    	Self::ParseSyntaxError => write!(f, "Syntax error"),
			Self::ParseDurationError => write!(f, "Could not parse duration"),
		}
  }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Duration(time::Duration);

impl Duration {
	pub fn duration<'a>(&'a self) -> &'a time::Duration {
		&self.0
	}
}

impl FromStr for Duration {
	type Err = Error;
	fn from_str(s: &str) -> Result<Self, Self::Err> {
		Ok(Self(parse(s)?))
	}
}

fn is_digit(c: char) -> bool {
  c.is_digit(10)
}

fn parse_value(input: &str) -> Result<u64, Error> {
  Ok(u64::from_str_radix(input, 10)?)
}

fn value_primary(input: &str) -> nom::IResult<&str, u64> {
  nom::combinator::map_res(
    nom::bytes::complete::take_while_m_n(1, 10, is_digit),
    parse_value
  )(input)
}

fn is_unit(c: char) -> bool {
  match c {
    'd' | 'h' | 'm' | 's' => true,
    _ => false,
  }
}

fn parse_unit(input: &str) -> Result<u64, Error> {
  match input {
    "d" => Ok(60 * 60 * 24), // naive day: 24 hours
    "h" => Ok(60 * 60),
    "m" => Ok(60),
    "s" => Ok(1),
    _   => Err(Error::ParseDurationError),
  }
}

fn unit_primary(input: &str) -> nom::IResult<&str, u64> {
  nom::combinator::map_res(
    nom::bytes::complete::take_while_m_n(1, 1, is_unit),
    parse_unit
  )(input)
}

fn unit_value(input: &str) -> nom::IResult<&str, u64> {
  let (input, (val, unit)) = nom::sequence::tuple((value_primary, unit_primary))(input)?;
  Ok((input, val * unit))
}

pub fn parse(input: &str) -> Result<time::Duration, Error> {
  let mut input = input.trim();
  if input.len() == 0 { // empty input is an error
    return Err(Error::ParseDurationError)
  }
  
  let mut result: u64 = 0;
  while input != "" {
    let (remainder, duration) = unit_value(input)?;
    result = result + duration;
    input = remainder;
  }
  
	Ok(time::Duration::from_secs(result))
}

#[cfg(test)]
mod tests {
  use super::*;
  
  #[test]
  fn test_parse_duration() {
    assert_eq!(Ok(time::Duration::from_secs(1)), parse("1s"));
    assert_eq!(Ok(time::Duration::from_secs(3600)), parse("1h"));
    assert_eq!(Ok(time::Duration::from_secs(86400)), parse("1d"));
    assert_eq!(Ok(time::Duration::from_secs(3660)), parse("1h1m"));
    assert_eq!(Ok(time::Duration::from_secs(7200)), parse("1h1h"));
    assert_eq!(Ok(time::Duration::from_secs(3660)), parse(" 1h1m "));
    assert_eq!(Err(Error::ParseDurationError), parse(""));
    assert_eq!(Err(Error::ParseDurationError), parse("   "));
    assert_eq!(Err(Error::ParseSyntaxError), parse("1"));
    assert_eq!(Err(Error::ParseSyntaxError), parse("s"));
  }
  
}

