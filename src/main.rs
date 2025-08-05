use scraper::{Html, Selector};
use std::collections::{HashSet, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use tokio::task::JoinSet;
use tracing::level_filters::LevelFilter;
use tracing::{error, info};
use tracing_subscriber::{EnvFilter, FmtSubscriber};
use url::Url;

#[derive(Debug)]
struct CrawlStats {
    successful_requests: u32,
    failed_requests: u32,
    total_urls_found: usize,
    start_time: Instant,
}

impl CrawlStats {
    fn new() -> Self {
        Self {
            successful_requests: 0,
            failed_requests: 0,
            total_urls_found: 0,
            start_time: Instant::now(),
        }
    }
}

#[derive(Debug)]
struct CrawlState {
    to_visit: Mutex<VecDeque<String>>,
    visited: Mutex<HashSet<String>>,
    stats: Mutex<CrawlStats>,
}

impl CrawlState {
    fn new(base_url: String) -> Self {
        let mut to_visit = VecDeque::new();
        to_visit.push_back(base_url);
        Self {
            to_visit: Mutex::new(to_visit),
            visited: Mutex::new(HashSet::new()),
            stats: Mutex::new(CrawlStats::new()),
        }
    }
}

fn find_all_urls(base_url_str: &str, html_body: &str) -> HashSet<String> {
    let base_url = match Url::parse(base_url_str) {
        Ok(url) => url,
        Err(e) => {
            error!("Could not parse base url {}: {}", base_url_str, e);
            return HashSet::new();
        }
    };
    let base_domain = base_url.domain().unwrap_or_default();
    let document = Html::parse_document(html_body);
    let selector = Selector::parse("a[href]").unwrap();

    document
        .select(&selector)
        .filter_map(|element| element.value().attr("href"))
        .filter_map(|href| base_url.join(href).ok())
        .filter(|url| url.domain().unwrap_or_default() == base_domain)
        .map(|mut url| {
            url.set_fragment(None);
            url.to_string()
        })
        .collect()
}

async fn crawl_task(
    url: String,
    client: reqwest::Client,
    state: Arc<CrawlState>,
) -> Result<(), reqwest::Error> {
    let resp = client.get(&url).send().await?;

    if !resp.status().is_success() {
        error!(
            "[-] Request to {} failed with status: {}",
            url,
            resp.status()
        );
        let mut stats = state.stats.lock().await;
        stats.failed_requests += 1;
        return Ok(());
    }

    let body = resp.text().await?;
    let new_urls = find_all_urls(&url, &body);
    let new_urls_count = new_urls.len();

    let mut queue = state.to_visit.lock().await;
    for new_url in new_urls {
        queue.push_back(new_url);
    }
    drop(queue);

    let mut stats = state.stats.lock().await;
    stats.successful_requests += 1;
    stats.total_urls_found += new_urls_count;

    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let filter = EnvFilter::builder()
        .with_env_var("PROBE_LOG")
        .with_default_directive(LevelFilter::INFO.into())
        .from_env_lossy();
    let subscriber = FmtSubscriber::builder().with_env_filter(filter).finish();
    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");

    let base_url = "https://crawler-test.com".to_string();
    let crawl_limit = 1000;
    const CONCURRENT_REQUESTS: usize = 20;

    let state = Arc::new(CrawlState::new(base_url.clone()));
    let client = reqwest::Client::builder()
        .user_agent("ProbeBot/1.0")
        .timeout(Duration::from_secs(10))
        .build()?;

    let mut tasks = JoinSet::new();

    info!("[+] BASE URL: {}", base_url);

    loop {
        let queue_is_empty = state.to_visit.lock().await.is_empty();
        if queue_is_empty && tasks.is_empty() {
            break;
        }

        while tasks.len() < CONCURRENT_REQUESTS {
            let mut queue = state.to_visit.lock().await;
            if queue.is_empty() {
                break;
            }
            let url = queue.pop_front().unwrap();
            drop(queue);

            let mut visited_set = state.visited.lock().await;
            if visited_set.len() >= crawl_limit {
                if tasks.is_empty() {
                    break;
                }
                continue;
            }
            if !visited_set.insert(url.clone()) {
                continue;
            }
            drop(visited_set);

            info!("[+] {}", url);
            tasks.spawn(crawl_task(url, client.clone(), Arc::clone(&state)));
        }

        if let Some(res) = tasks.join_next().await {
            if let Err(e) = res? {
                error!("Error during crawling: {}", e);
            }
        }
    }

    let stats = state.stats.lock().await;
    let visited_count = state.visited.lock().await.len();
    let total_duration = stats.start_time.elapsed();

    info!("[+] Crawl Finished");
    info!("[=] Total time: {:.2?}", total_duration);
    info!("[=] Visited {} unique URLs.", visited_count);
    info!("[=] Successful requests: {}", stats.successful_requests);
    info!("[=] Failed requests: {}", stats.failed_requests);
    info!("[=] Total URLs found: {}", stats.total_urls_found);

    Ok(())
}
