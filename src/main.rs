use scraper::{Html, Selector};
use std::collections::{HashSet, VecDeque};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tokio::task::JoinSet;
use tracing::level_filters::LevelFilter;
use tracing::{error, info};
use tracing_subscriber::{EnvFilter, FmtSubscriber};

fn find_all_urls(base_url: &str, html_body: &str) -> HashSet<String> {
    let document = Html::parse_document(html_body);
    let selector = Selector::parse("a").unwrap();
    let mut urls = HashSet::new();

    let base_url = base_url.trim_end_matches('/');

    for element in document.select(&selector) {
        if let Some(href) = element.value().attr("href") {
            match url::Url::parse(base_url) {
                Ok(base) => {
                    if let Ok(absolute_url) = base.join(href) {
                        urls.insert(absolute_url.to_string());
                    }
                }
                Err(e) => {
                    error!("Could not parse base url {}: {}", base_url, e);
                }
            }
        }
    }
    urls
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let filter = EnvFilter::builder()
        .with_env_var("PROBE_LOG")
        .with_default_directive(LevelFilter::INFO.into())
        .from_env_lossy();

    let subscriber = FmtSubscriber::builder().with_env_filter(filter).finish();

    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");

    let base_url = "https://crawler-test.com";
    let limit = 100;
    const CONCURRENT_REQUESTS: usize = 20;

    let to_visit = Arc::new(Mutex::new(VecDeque::new()));
    let visited = Arc::new(Mutex::new(HashSet::new()));

    to_visit.lock().await.push_back(base_url.to_owned());

    let client = reqwest::Client::builder()
        .user_agent("ProbeBot/1.0")
        .timeout(Duration::from_secs(10))
        .build()?;

    let mut tasks = JoinSet::new();

    info!("[+] BASE URL: {}", base_url);

    loop {
        if to_visit.lock().await.is_empty() && tasks.is_empty() {
            break;
        }

        while tasks.len() < CONCURRENT_REQUESTS {
            let mut queue = to_visit.lock().await;
            if queue.is_empty() {
                break;
            }
            let url = queue.pop_front().unwrap();
            drop(queue);

            let mut visited_set = visited.lock().await;
            if visited_set.contains(&url) || visited_set.len() >= limit {
                continue;
            }
            visited_set.insert(url.clone());
            drop(visited_set);

            info!("[+] {}", url);

            let to_visit_clone = Arc::clone(&to_visit);
            let client_clone = client.clone();

            tasks.spawn(async move {
                let resp = client_clone.get(&url).send().await?;
                let body = resp.text().await?;
                let new_urls = find_all_urls(&url, &body);

                let mut queue = to_visit_clone.lock().await;
                for new_url in new_urls {
                    queue.push_back(new_url);
                }
                Ok::<(), reqwest::Error>(())
            });
        }

        if let Some(res) = tasks.join_next().await {
            if let Err(e) = res? {
                error!("Error during crawling: {}", e);
            }
        }
    }

    info!("[+] Crawl Finished");
    let final_visited = visited.lock().await;
    info!("[=] Visited {} unique URLs.", final_visited.len());
    // info!("[=] Visited URLs: {:#?}", *final_visited);
    Ok(())
}
