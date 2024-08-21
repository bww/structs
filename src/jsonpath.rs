use serde_json;
use serde_json::value::Value;

const SEP: &str = ".";

#[derive(Debug, Clone, PartialEq)]
pub struct Path(String);

impl Path {
  pub fn new(path: &str) -> Self {
    Self(path.to_string())
  }

  pub fn _path<'a>(&'a self) -> &'a str {
    &self.0
  }

  pub fn value<'a>(&self, value: &'a Value) -> Option<&'a Value> {
    match self.find(value) {
      (Some(v), None) => Some(v),
      _               => None,
    }
  }

  pub fn find<'a>(&self, value: &'a Value) -> (Option<&'a Value>, Option<Path>) {
    let (v, p) = self.deref(value);
    match v {
      Some(v) => match p {
        Some(p) => p.find(v),       // deref and continue
        None    => (Some(v), None), // found, done
      },
      None => match p {
        Some(p) => (None, Some(p)), // ran out of values before path elements; not found
        None    => (None, None),    // no value, no path; not found
      },
    }
  }

  pub fn deref<'a>(&self, value: &'a Value) -> (Option<&'a Value>, Option<Path>) {
    let (n, r) = self.next();
    let v = match n {
      Some(n) => json_deref(n, value),
      None    => None,
    };
    let r = match r {
      Some(r) => Some(Path::new(r)),
      None    => None,
    };
    match v {
      Some(v) => (Some(v), r),
      None    => (None, Some(self.clone())),
    }
  }

  pub fn has_next(&self) -> bool {
    match self.0.find(SEP) {
      Some(_) => true,
      None    => false,
    }
  }

  pub fn next<'a>(&'a self) -> (Option<&'a str>, Option<&'a str>) {
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

  pub fn trim<'a>(&'a self, c: usize) -> (Option<Path>, Option<&'a str>) {
    let mut p: &str = &self.0;
    let mut n: Option<&str> = None;
    for _ in 0..c {
      match p.rfind(SEP) {
        Some(x) => {
          n = Some(&p[x+1..]);
          p = &p[..x];
        },
        None => return (None, None),
      }
    }
    (Some(Path::new(p)), n)
  }
}

pub fn print_raw<'a>(value: &'a Value) -> String {
  match value {
    Value::Null      => "null".to_string(),
    Value::Bool(v)   => format!("{}", v),
    Value::Number(v) => format!("{}", v),
    Value::String(v) => format!("{}", v),
    Value::Array(_)  => value.to_string(),
    Value::Object(_) => value.to_string(),
  }
}

fn json_deref<'a>(name: &str, value: &'a Value) -> Option<&'a Value> {
  match value {
    Value::Null      => None,
    Value::Bool(_)   => None,
    Value::Number(_) => None,
    Value::String(_) => None,
    Value::Object(v) => v.get(name),
    Value::Array(v)  => match name.parse::<usize>() {
      Ok(i) => if i < v.len() {
        Some(&v[i])
      } else {
        None
      },
      Err(_) => None,
    },
  }
}

pub fn index(value: &Value, name: &str) -> Option<usize> {
  if let Value::Array(value) = value {
    index_array(value, name)
  }else{
    None
  }
}

pub fn index_array(value: &Vec<Value>, name: &str) -> Option<usize> {
  let i = match name.parse::<usize>() {
    Ok(i)  => i,
    Err(_) => return None,
  };
  if value.len() > i {
    Some(i)
  }else{
    None
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn trim_path() {
    let p = Path::new("a");
    assert_eq!((Some(Path::new("a")), None), p.trim(0));
    let p = Path::new("a.b");
    assert_eq!((Some(Path::new("a")), Some("b")), p.trim(1));
    let p = Path::new("a.b.c");
    assert_eq!((Some(Path::new("a.b")), Some("c")), p.trim(1));
    let p = Path::new("a.b.c");
    assert_eq!((Some(Path::new("a")), Some("b")), p.trim(2));
    let p = Path::new("a.b.c");
    assert_eq!((None, None), p.trim(3));
    let p = Path::new("a.b.c");
    assert_eq!((None, None), p.trim(100));
  }

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
