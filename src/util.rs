/// Threading utilities
pub mod thread {
    /// Like `thread::spawn`, but with a `name` argument
    pub fn spawn_named<F, T, S>(name: S, f: F) -> ::std::thread::JoinHandle<T>
        where F: FnOnce() -> T,
              F: Send + 'static,
              T: Send + 'static,
              S: Into<String>
    {
        ::std::thread::Builder::new().name(name.into()).spawn(f).expect("thread spawn works")
    }
}
