extern crate copypasta;

use copypasta::ClipboardContext;
use copypasta::ClipboardProvider;

fn main() {
    let mut ctx = ClipboardContext::new().unwrap();

    let the_string = "Hello, world!";

    ctx.set_contents(the_string.to_owned()).unwrap();
}
