
macro_rules! logln {
  () => {
    eprintln!()
  };
  ($fmt:literal) => {
    eprintln!("[{}] {}", std::process::id(), $fmt)
  };
  ($fmt:literal, $($arg:tt)*) => {
    eprintln!("[{}] {}", std::process::id(), format!($fmt, $($arg)*))
  };
}

pub(crate) use logln;
