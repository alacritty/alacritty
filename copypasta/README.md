# copypasta

copypasta is a [rust-clipboard](https://github.com/aweinstock314/rust-clipboard) fork, adding support for the Wayland clipboard.

rust-clipboard is a cross-platform library for getting and setting the contents of the OS-level clipboard.  

## Example

```rust
extern crate copypasta;

use copypasta::ClipboardContext;

fn example() {
    let mut ctx = ClipboardContext::new().unwrap();
    println!("{:?}", ctx.get_contents());
    ctx.set_contents("some string".to_owned()).unwrap();
}
```

## API

The `ClipboardProvider` trait has the following functions:

```rust
fn get_contents(&mut self) -> Result<String, Box<Error>>;
fn set_contents(&mut self, String) -> Result<(), Box<Error>>;
```

`ClipboardContext` is a type alias for one of {`WindowsClipboardContext`, `OSXClipboardContext`, `X11ClipboardContext`, `NopClipboardContext`}, all of which implement `ClipboardProvider`. Which concrete type is chosen for `ClipboardContext` depends on the OS (via conditional compilation).

## License

`rust-clipboard` is dual-licensed under MIT and Apache2.
