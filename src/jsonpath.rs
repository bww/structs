use serde_json;
use serde_json::value::Value;

const SEP: &str = ".";

#[derive(Debug, Clone, PartialEq)]
pub struct Path(String);

impl Path {
	pub fn new(path: &str) -> Self {
		Self(path.to_string())
	}

	pub fn find<'a>(&self, value: &'a Value) -> (Option<&'a Value>, Option<Path>) {
		let (v, p) = self.deref(value);
		match v {
			Some(v) => match p {
				Some(p) => p.find(v),				// deref and continue
				None		=> (Some(v), None), // found, done
			},
			None => match p {
				Some(p) => (None, Some(p)), // ran out of values before path elements; not found
				None		=> (None, None),		// no value, no path; not found
			},
		}
	}

	pub fn deref<'a>(&self, value: &'a Value) -> (Option<&'a Value>, Option<Path>) {
		let (n, r) = self.next();
		let v = match n {
			Some(n) => json_deref(n, value),
			None 		=> None,
		};
		let r = match r {
			Some(r) => Some(Path::new(r)),
			None 		=> None,
		};
		match v {
			Some(v) => (Some(v), r),
			None		=> (None, Some(self.clone())),
		}
	}

	fn next<'a>(&'a self) -> (Option<&'a str>, Option<&'a str>) {
		match self.0.find(SEP) {
			Some(x) => (Some(&self.0[..x]), if self.0.len() > x {
				Some(&self.0[x+1..])
			} else {
				None
			}),
			None => (if self.0.len() > 0 {
				Some(&self.0)
			} else {
				None
			}, None),
		}
	}
}

fn json_deref<'a>(name: &str, value: &'a Value) -> Option<&'a Value> {
	match value {
		Value::Null 		 => None,
		Value::Bool(_) 	 => None,
		Value::Number(_) => None,
		Value::String(_) => None,
		Value::Array(_)  => None,
		Value::Object(v) => v.get(name),
	}
}

#[cfg(test)]
mod tests {
  use super::*;
  
  #[test]
  fn find_path() {
		let v: Value = serde_json::from_str(r#"{
			"str": "Hello!",
			"arr": [1, 2, 3],
			"num": 123,
			"bool": true,
			"sub1": {
				"a": 1,
				"b": 2,
				"c": 3
			},
			"sub2": {
				"A": {"one": 1},
				"B": {"two": 2},
				"C": {"three": 3}
			}
		}"#).unwrap();

		let p = Path::new("str");
    assert_eq!((Some(&Value::String("Hello!".to_string())), None), p.deref(&v));
		let p = Path::new("str.invalid");
    assert_eq!((Some(&Value::String("Hello!".to_string())), Some(Path::new("invalid"))), p.deref(&v));
		let p = Path::new("num");
    assert_eq!((Some(&Value::Number(123.into())), None), p.deref(&v));
		let p = Path::new("num.invalid");
    assert_eq!((Some(&Value::Number(123.into())), Some(Path::new("invalid"))), p.deref(&v));
		let p = Path::new("num.invalid.nonsense");
    assert_eq!((Some(&Value::Number(123.into())), Some(Path::new("invalid.nonsense"))), p.deref(&v));

		let p = Path::new("str.invalid");
    assert_eq!((None, Some(Path::new("invalid"))), p.find(&v));
		let p = Path::new("num.invalid");
    assert_eq!((None, Some(Path::new("invalid"))), p.find(&v));
		let p = Path::new("sub1.a");
    assert_eq!((Some(&Value::Number(1.into())), None), p.find(&v));
		let p = Path::new("sub1.a.invalid");
    assert_eq!((None, Some(Path::new("invalid"))), p.find(&v));
		let p = Path::new("sub1.a.invalid.nonsense");
    assert_eq!((None, Some(Path::new("invalid.nonsense"))), p.find(&v));

		let p = Path::new("sub2.A.one");
    assert_eq!((Some(&Value::Number(1.into())), None), p.find(&v));
		let p = Path::new("sub2.B.two");
    assert_eq!((Some(&Value::Number(2.into())), None), p.find(&v));
		let p = Path::new("sub2.B.two.invalid");
    assert_eq!((None, Some(Path::new("invalid"))), p.find(&v));
		let p = Path::new("sub2.B.two.invalid.nonsense");
    assert_eq!((None, Some(Path::new("invalid.nonsense"))), p.find(&v));

  }
}
