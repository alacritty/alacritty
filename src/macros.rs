#[macro_export]
macro_rules! die {
    ($($arg:tt)*) => {
        println!($($arg)*);
        ::std::process::exit(1);
    }
}

#[macro_export]
macro_rules! err_println {
    ($($arg:tt)*) => {
        if let Err(_) = writeln!(&mut ::std::io::stderr(), $($arg)*) {
            die!("Cannot reach stderr");
        }
    }
}
