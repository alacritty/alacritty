extern crate copypasta;

use copypasta::ClipboardProvider;
use copypasta::ClipboardContext;

fn main() {
    let mut ctx = ClipboardContext::new().unwrap();

    let the_string = "Hello, world!";

    ctx.set_contents(the_string.to_owned()).unwrap();
}
