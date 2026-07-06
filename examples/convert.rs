//! One-call convert CLI.
//!
//! ```sh
//! API2CONVERT_API_KEY=… cargo run --example convert -- <input> <target> <out>
//! # e.g.
//! API2CONVERT_API_KEY=… cargo run --example convert -- photo.heic jpg out/
//! API2CONVERT_API_KEY=… cargo run --example convert -- https://example.com/a.png jpg a.jpg
//! ```

use std::process::exit;

use api2convert::Api2Convert;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 4 {
        eprintln!("usage: convert <input-path-or-url> <target-format> <output-path-or-dir>");
        exit(2);
    }
    let (input, target, out) = (args[1].as_str(), args[2].as_str(), args[3].as_str());

    let client = match Api2Convert::from_env() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("client error: {e}");
            exit(1);
        }
    };

    let result = match client.convert(input, target) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("conversion failed: {e}");
            exit(1);
        }
    };

    match result.save(out, None) {
        Ok(path) => println!("saved {}", path.display()),
        Err(e) => {
            eprintln!("download failed: {e}");
            exit(1);
        }
    }
}
