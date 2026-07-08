//! Audio operations guide — transcode a WAV file to AAC with explicit options.
//!
//! ```sh
//! API2CONVERT_API_KEY=… cargo run --example audio-operations
//! ```

use api2convert::{Api2Convert, ConvertOptions};

const WAV: &str = "https://example-files.online-convert.com/audio/wav/example.wav";

/// Build a client from `API2CONVERT_API_KEY` (and optional `API2CONVERT_BASE_URL`).
fn client() -> Api2Convert {
    let mut builder = Api2Convert::builder();
    if let Ok(base) = std::env::var("API2CONVERT_BASE_URL") {
        if !base.is_empty() {
            builder = builder.base_url(base);
        }
    }
    builder
        .build()
        .expect("set API2CONVERT_API_KEY (and optionally API2CONVERT_BASE_URL)")
}

fn main() -> Result<(), api2convert::Api2ConvertError> {
    let client = client();

    let result = client.convert_with(
        WAV,
        "aac",
        ConvertOptions::new()
            .category("audio")
            .option("audio_codec", "aac")
            .option("audio_bitrate", 192)
            .option("channels", "stereo")
            .option("frequency", 44100),
    )?;
    println!("job {} is {}", result.job().id, result.job().status.code);

    let path = result.save("out/", None)?;
    println!("saved {}", path.display());

    Ok(())
}
