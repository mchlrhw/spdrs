use anyhow::{bail, Result};
use once_cell::sync::Lazy;
use regex::Regex;
use std::env;

static LINK_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"href="(https?://.+?)""#).expect("we must have written a valid regex")
});

fn extract_links(text: &str) -> Vec<&str> {
    LINK_REGEX
        .captures_iter(text)
        .map(|c| {
            let (_, [link]) = c.extract();

            link
        })
        .collect()
}

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
    for link in extract_links(&resp_text) {
        println!("  * {link}")
    }
    println!();

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_links() {
        let text = "nothing to see here";
        let expected: Vec<&str> = vec![];

        let links = extract_links(text);

        assert_eq!(links, expected);
    }

    #[test]
    fn single_link() {
        let text = r#"href="https://wikipedia.org""#;
        let expected: Vec<&str> = vec!["https://wikipedia.org"];

        let links = extract_links(text);

        assert_eq!(links, expected);
    }

    #[test]
    fn html_from_the_wild() {
        let text = include_str!("../resources/test-data/wikipedia.org.html");
        let expected: Vec<&str> = vec![
            "https://wikis.world/@wikipedia",
            "https://meta.wikimedia.org/wiki/Special:MyLanguage/List_of_Wikipedias",
            "https://donate.wikimedia.org/?utm_medium=portal&utm_campaign=portalFooter&utm_source=portalFooter",
            "https://en.wikipedia.org/wiki/List_of_Wikipedia_mobile_applications",
            "https://play.google.com/store/apps/details?id=org.wikipedia&referrer=utm_source%3Dportal%26utm_medium%3Dbutton%26anid%3Dadmob",
            "https://itunes.apple.com/app/apple-store/id324715238?pt=208305&ct=portal&mt=8",
            "https://creativecommons.org/licenses/by-sa/4.0/",
            "https://meta.wikimedia.org/wiki/Terms_of_use",
            "https://meta.wikimedia.org/wiki/Privacy_policy",
        ];

        let links = extract_links(text);

        assert_eq!(links, expected);
    }
}
