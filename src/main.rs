use anyhow::{anyhow, bail, Result};
use async_recursion::async_recursion;
use once_cell::sync::Lazy;
use scraper::{Html, Selector};
use std::{
    collections::HashSet,
    env,
    sync::{Arc, Mutex},
};
use tokio::{
    sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender},
    task,
};
use tracing::{debug, trace, warn};
use tracing_subscriber::EnvFilter;
use url::Url;

static SEEN: Lazy<Arc<Mutex<HashSet<String>>>> = Lazy::new(Arc::default);

#[derive(Debug, PartialEq)]
struct CrawlData {
    url: String,
    links: HashSet<String>,
}

async fn fetch(url: Url) -> Result<String> {
    let resp_text = reqwest::get(url).await?.error_for_status()?.text().await?;

    Ok(resp_text)
}

fn extract_links(text: &str) -> HashSet<String> {
    let mut links = HashSet::new();
    let a_selector = Selector::parse("a").expect("we can parse anchor links");
    let li_selector = Selector::parse("link").expect("we can parse links");

    let html = Html::parse_document(text);
    for element in html.select(&a_selector).chain(html.select(&li_selector)) {
        if let Some(link) = element.attr("href") {
            links.insert(link.to_string());
        }
    }

    links
}

fn filter_external(links: HashSet<String>, allowed_subdomain: &str) -> HashSet<String> {
    links
        .into_iter()
        .filter(|l| {
            l.starts_with(&format!("http://{allowed_subdomain}"))
                || l.starts_with(&format!("https://{allowed_subdomain}"))
        })
        .collect()
}

fn resolve_relative_paths(base: &Url, links: HashSet<String>) -> HashSet<String> {
    links
        .into_iter()
        .map(|l| match base.join(&l) {
            Ok(url) => url.to_string(),
            _ => l,
        })
        .collect()
}

fn resolve_relative_schemes(base: &Url, links: HashSet<String>) -> HashSet<String> {
    links
        .into_iter()
        .map(|l| {
            if l.starts_with("//") {
                match Url::parse(&format!("{}:{l}", base.scheme())) {
                    Ok(url) => url.to_string(),
                    _ => l,
                }
            } else {
                l
            }
        })
        .collect()
}

#[async_recursion]
async fn crawl(
    url: Url,
    allowed_subdomain: String,
    print_channel: UnboundedSender<CrawlData>,
) -> Result<()> {
    debug!("fetching {url}");
    let resp_text = fetch(url.clone()).await?;
    trace!("received");

    let links = extract_links(&resp_text);
    debug!("extracted {links:?}");
    let resolved_schemes = resolve_relative_schemes(&url, links);
    let resolved_paths = resolve_relative_paths(&url, resolved_schemes);
    let filtered = filter_external(resolved_paths, &allowed_subdomain);
    debug!("filtered down to {filtered:?}");

    let crawl_data = CrawlData {
        url: url.to_string(),
        links: filtered.iter().map(ToString::to_string).collect(),
    };

    debug!("sending crawl data for {url}");
    print_channel.send(crawl_data)?;

    SEEN.lock().unwrap().insert(url.to_string());

    for link in filtered {
        let url = match Url::parse(&link) {
            Ok(url) => url,
            Err(error) => {
                warn!("Error parsing {link} ({error})");

                continue;
            }
        };

        debug!("checking seen for {link}");
        if SEEN.lock().unwrap().contains(&link.to_string()) {
            debug!("seen {link}, skipping...");
            continue;
        }

        debug!("not seen {link} yet, crawling...");
        task::spawn(crawl(url, allowed_subdomain.clone(), print_channel.clone()));
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
        println!();
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        bail!("Usage: spdrs <url>");
    }

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .init();

    let url_str = args
        .get(1)
        .expect("the index must exist due to previous len check");

    let url = Url::parse(url_str)?;
    let allowed_subdomain = url.host_str().ok_or(anyhow!("Missing host"))?;
    debug!("restricting links to {allowed_subdomain}");

    let (snd, rcv) = unbounded_channel();
    let task_handle = task::spawn(async move { printer(rcv).await });

    crawl(url.clone(), allowed_subdomain.to_string(), snd).await?;

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
        let text = r#"<a href="https://wikipedia.org">Link</a>"#;
        let expected = HashSet::from_iter(["https://wikipedia.org".to_string()]);

        let links = extract_links(text);

        assert_eq!(links, expected);
    }

    #[test]
    fn simple_html() {
        let text = r#"
<a href="https://wikipedia.org"/>
<a href="https://wikipedia.org/index.html"/>
"#;
        let expected = HashSet::from_iter([
            "https://wikipedia.org".to_string(),
            "https://wikipedia.org/index.html".to_string(),
        ]);

        let links = extract_links(text);

        assert_eq!(links, expected);
    }

    #[test]
    fn html_with_multiple_links_to_a_line() {
        let text =
            r#"<a href="https://wikipedia.org"/><a href="https://wikipedia.org/index.html"/>"#;
        let expected = HashSet::from_iter([
            "https://wikipedia.org".to_string(),
            "https://wikipedia.org/index.html".to_string(),
        ]);

        let links = extract_links(text);

        assert_eq!(links, expected);
    }

    #[test]
    fn filter_external_links() {
        let allowed_subdomain = "example.com";
        let links = HashSet::from_iter([
            "http://example.com".to_string(),
            "https://example.com/foo.jpg".to_string(),
            "http://wikipedia.org/bar.png".to_string(),
            "https://wikipedia.org/baz.gif".to_string(),
        ]);
        let expected = HashSet::from_iter([
            "http://example.com".to_string(),
            "https://example.com/foo.jpg".to_string(),
        ]);

        let filtered = filter_external(links, allowed_subdomain);

        assert_eq!(filtered, expected);
    }

    #[test]
    fn relative_path_links_can_be_resolved() {
        let url = Url::parse("https://example.com/dir/").expect("test URL should parse");
        let links = HashSet::from_iter([
            "foo.jpg".to_string(),
            "bar.png".to_string(),
            "../baz.gif".to_string(),
        ]);
        let expected = HashSet::from_iter([
            "https://example.com/dir/foo.jpg".to_string(),
            "https://example.com/dir/bar.png".to_string(),
            "https://example.com/baz.gif".to_string(),
        ]);

        let resolved = resolve_relative_paths(&url, links);

        assert_eq!(resolved, expected);
    }

    #[test]
    fn relative_scheme_links_can_be_resolved() {
        let url = Url::parse("https://example.com").expect("test URL should parse");
        let links = HashSet::from_iter([
            "//www.example.com/".to_string(),
            "//example.com/foo.png".to_string(),
            "//wikipedia.org".to_string(),
        ]);
        let expected = HashSet::from_iter([
            "https://www.example.com/".to_string(),
            "https://example.com/foo.png".to_string(),
            "https://wikipedia.org/".to_string(),
        ]);

        let resolved = resolve_relative_schemes(&url, links);

        assert_eq!(resolved, expected);
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
        let url = Url::parse("http://localhost:8000/").expect("test URL is parseable");
        let res = fetch(url).await;

        assert!(res.is_ok());
    }

    #[tokio::test]
    async fn no_links() {
        let (snd, rcv) = unbounded_channel();
        let allowed_subdomain = "localhost:8000".to_string();
        let url = Url::parse("http://localhost:8000/no-links.html").expect("test URL is parseable");

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
        let url =
            Url::parse("http://localhost:8000/recursive.html").expect("test URL is parseable");

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
