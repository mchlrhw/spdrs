use anyhow::{anyhow, bail, Result};
use async_recursion::async_recursion;
use once_cell::sync::Lazy;
use regex::Regex;
use std::{
    collections::HashSet,
    env,
    sync::{Arc, Mutex},
};
use tokio::{
    sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender},
    task,
};
use tracing::{debug, trace};
use url::Url;

static LINK_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"href="(https?://.+?)""#).expect("we must have written a valid regex")
});
static SEEN: Lazy<Arc<Mutex<HashSet<String>>>> = Lazy::new(Arc::default);

#[derive(Debug, PartialEq)]
struct CrawlData {
    url: String,
    links: HashSet<String>,
}

async fn fetch(url: &str) -> Result<String> {
    let resp_text = reqwest::get(url).await?.error_for_status()?.text().await?;

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
async fn crawl(
    url: String,
    allowed_subdomain: String,
    print_channel: UnboundedSender<CrawlData>,
) -> Result<()> {
    debug!("fetching {url}");
    let resp_text = fetch(&url).await?;
    trace!("received");

    let links = extract_links(&resp_text);
    debug!("extracted {links:?}");
    let filtered = filter_external(&links, &allowed_subdomain);
    debug!("filtered down to {filtered:?}");

    let crawl_data = CrawlData {
        url: url.to_string(),
        links: filtered.iter().map(|s| s.to_string()).collect(),
    };

    debug!("sending crawl data for {url}");
    print_channel.send(crawl_data)?;

    SEEN.lock().unwrap().insert(url.to_string());

    for link in filtered.into_iter() {
        debug!("checking seen for {link}");
        if SEEN.lock().unwrap().contains(&link.to_string()) {
            debug!("seen {link}, skipping...");
            continue;
        } else {
            debug!("not seen {link} yet, crawling...");
        }

        task::spawn(crawl(
            link.to_owned(),
            allowed_subdomain.to_owned(),
            print_channel.clone(),
        ));
    }

    Ok(())
}

async fn printer(mut print_channel: UnboundedReceiver<CrawlData>) {
    while let Some(data) = print_channel.recv().await {
        let CrawlData { url, links } = data;
        debug!("printer received crawl data for {url}");

        println!("{url}");
        for link in links {
            println!("  * {link}");
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        bail!("Usage: spdrs <url>");
    }

    tracing_subscriber::fmt()
        .with_env_filter("spdrs=debug")
        .with_writer(std::io::stderr)
        .init();

    let url = args
        .get(1)
        .expect("the index must exist due to previous len check");

    let parsed_url = Url::parse(url)?;
    let allowed_subdomain = parsed_url.host_str().ok_or(anyhow!("Missing host"))?;
    debug!("restricting links to {allowed_subdomain}");

    let (snd, rcv) = unbounded_channel();
    let task_handle = task::spawn(async move { printer(rcv).await });

    crawl(url.to_owned(), allowed_subdomain.to_string(), snd).await?;

    task_handle.await.unwrap();

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
    fn simple_html() {
        let text = r#"
<a href="https://wikipedia.org"/>
<a href="https://wikipedia.org/index.html"/>
"#;
        let expected =
            HashSet::from_iter(["https://wikipedia.org", "https://wikipedia.org/index.html"]);

        let links = extract_links(text);

        assert_eq!(links, expected);
    }

    #[test]
    fn html_with_multiple_links_to_a_line() {
        let text =
            r#"<a href="https://wikipedia.org"/><a href="https://wikipedia.org/index.html"/>"#;
        let expected =
            HashSet::from_iter(["https://wikipedia.org", "https://wikipedia.org/index.html"]);

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

#[cfg(all(test, feature = "e2e"))]
mod e2e_tests {
    use super::*;

    async fn receive_crawl_data(mut rcv: UnboundedReceiver<CrawlData>) -> Vec<CrawlData> {
        let mut crawl_data = vec![];
        while let Some(data) = rcv.recv().await {
            crawl_data.push(data);
        }

        crawl_data
    }

    #[tokio::test]
    async fn fetch_local_root() {
        let res = fetch("http://localhost:8000/").await;

        assert!(res.is_ok());
    }

    #[tokio::test]
    async fn no_links() {
        let (snd, rcv) = unbounded_channel();
        let allowed_subdomain = "localhost:8000".to_string();
        let url = "http://localhost:8000/no-links.html".to_string();

        let expected = vec![CrawlData {
            url: "http://localhost:8000/no-links.html".to_string(),
            links: HashSet::new(),
        }];

        let res = crawl(url, allowed_subdomain, snd).await;
        assert!(res.is_ok());

        let crawl_data = receive_crawl_data(rcv).await;

        assert_eq!(crawl_data, expected);
    }

    #[tokio::test]
    async fn recursive() {
        let (snd, rcv) = unbounded_channel();
        let allowed_subdomain = "localhost:8000".to_string();
        let url = "http://localhost:8000/recursive.html".to_string();

        let expected = vec![CrawlData {
            url: "http://localhost:8000/recursive.html".to_string(),
            links: HashSet::from_iter(["http://localhost:8000/recursive.html".to_string()]),
        }];

        let res = crawl(url, allowed_subdomain, snd).await;
        assert!(res.is_ok());

        let crawl_data = receive_crawl_data(rcv).await;

        assert_eq!(crawl_data, expected);
    }
}
