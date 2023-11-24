use anyhow::{bail, Result};
use std::env;

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        bail!("Usage: spdrs <url>");
    }

    let url = args
        .get(1)
        .expect("the index must exist due to previous len check");

    println!("{url}");

    let resp_text = reqwest::get(url).await?.text().await?;

    println!("{resp_text}");

    Ok(())
}
