use anyhow::{anyhow, bail, Result};
use async_recursion::async_recursion;
use once_cell::sync::Lazy;
use regex::Regex;
use std::{
    collections::HashSet,
    env,
    sync::{Arc, Mutex},
};
use url::Url;

static LINK_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"href="(https?://.+?)""#).expect("we must have written a valid regex")
});
static SEEN: Lazy<Arc<Mutex<HashSet<String>>>> = Lazy::new(Arc::default);

async fn fetch(url: &str) -> Result<String> {
    let resp_text = reqwest::get(url).await?.text().await?;

    Ok(resp_text)
}

fn extract_links(text: &str) -> HashSet<&str> {
    LINK_REGEX
        .captures_iter(text)
        .map(|c| {
            let (_, [link]) = c.extract();

            link
        })
        .collect()
}

fn filter_external<'l>(links: &HashSet<&'l str>, allowed_subdomain: &str) -> HashSet<&'l str> {
    links
        .iter()
        .filter(|l| {
            l.starts_with(&format!("http://{allowed_subdomain}"))
                || l.starts_with(&format!("https://{allowed_subdomain}"))
        })
        .copied()
        .collect()
}

#[async_recursion]
async fn crawl(url: &str, allowed_subdomain: &str) -> Result<()> {
    println!("{url}");

    let resp_text = fetch(url).await?;
    let links = extract_links(&resp_text);
    let filtered = filter_external(&links, allowed_subdomain);

    SEEN.lock().unwrap().insert(url.to_string());

    for link in &filtered {
        println!("  * {link}")
    }
    println!();

    for link in &filtered {
        if SEEN.lock().unwrap().contains(&link.to_string()) {
            continue;
        }

        crawl(link, allowed_subdomain).await?;
    }

    Ok(())
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

    let parsed_url = Url::parse(url)?;
    let allowed_subdomain = parsed_url.host_str().ok_or(anyhow!("Missing host"))?;

    crawl(url, allowed_subdomain).await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_links() {
        let text = "nothing to see here";
        let expected = HashSet::new();

        let links = extract_links(text);

        assert_eq!(links, expected);
    }

    #[test]
    fn single_link() {
        let text = r#"href="https://wikipedia.org""#;
        let expected = HashSet::from_iter(["https://wikipedia.org"]);

        let links = extract_links(text);

        assert_eq!(links, expected);
    }

    #[test]
    fn html_from_the_wild() {
        let text = include_str!("../resources/test-data/wikipedia.org.html");
        let expected = HashSet::from_iter([
            "https://wikis.world/@wikipedia",
            "https://meta.wikimedia.org/wiki/Special:MyLanguage/List_of_Wikipedias",
            "https://donate.wikimedia.org/?utm_medium=portal&utm_campaign=portalFooter&utm_source=portalFooter",
            "https://en.wikipedia.org/wiki/List_of_Wikipedia_mobile_applications",
            "https://play.google.com/store/apps/details?id=org.wikipedia&referrer=utm_source%3Dportal%26utm_medium%3Dbutton%26anid%3Dadmob",
            "https://itunes.apple.com/app/apple-store/id324715238?pt=208305&ct=portal&mt=8",
            "https://creativecommons.org/licenses/by-sa/4.0/",
            "https://meta.wikimedia.org/wiki/Terms_of_use",
            "https://meta.wikimedia.org/wiki/Privacy_policy",
        ]);

        let links = extract_links(text);

        assert_eq!(links, expected);
    }

    #[test]
    fn filter_external_links() {
        let allowed_subdomain = "example.com";
        let links = HashSet::from_iter([
            "http://example.com",
            "https://example.com/foo.jpg",
            "http://wikipedia.org/bar.png",
            "https://wikipedia.org/baz.gif",
        ]);
        let expected = HashSet::from_iter(["http://example.com", "https://example.com/foo.jpg"]);

        let filtered = filter_external(&links, allowed_subdomain);

        assert_eq!(filtered, expected);
    }
}
