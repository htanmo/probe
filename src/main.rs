use clap::Parser;
use reqwest::StatusCode;
use scraper::{Html, Selector};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, RwLock, Semaphore};
use tracing::{debug, error, info, warn};
use tracing_subscriber::{EnvFilter, FmtSubscriber};
use url::Url;

mod cli;

#[derive(Debug)]
struct CrawlStats {
    successful_requests: u64,
    failed_requests: u64,
    total_urls_found: u64,
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
    to_visit: Mutex<VecDeque<(String, usize)>>,
    visited: RwLock<HashSet<String>>,
    stats: Mutex<CrawlStats>,
    robots_cache: Mutex<HashMap<String, Vec<String>>>,
    domain_next_allowed: Mutex<HashMap<String, Instant>>,
}

impl CrawlState {
    fn new(seed: String) -> Self {
        let mut dq = VecDeque::new();
        dq.push_back((seed, 0));
        Self {
            to_visit: Mutex::new(dq),
            visited: RwLock::new(HashSet::new()),
            stats: Mutex::new(CrawlStats::new()),
            robots_cache: Mutex::new(HashMap::new()),
            domain_next_allowed: Mutex::new(HashMap::new()),
        }
    }
}

fn find_all_urls(base_url_str: &str, html_body: &str) -> HashSet<String> {
    let base_url = match Url::parse(base_url_str) {
        Ok(u) => u,
        Err(e) => {
            error!("Could not parse base url {}: {}", base_url_str, e);
            return Default::default();
        }
    };
    let base_domain = base_url.domain().map(|d| d.to_string());

    let document = Html::parse_document(html_body);
    let selector = match Selector::parse("a[href]") {
        Ok(s) => s,
        Err(e) => {
            error!("Selector parse error: {}", e);
            return Default::default();
        }
    };

    document
        .select(&selector)
        .filter_map(|el| el.value().attr("href"))
        .filter_map(|href| base_url.join(href).ok())
        .filter(|url| url.scheme().starts_with("http"))
        .filter(|url| match (&base_domain, url.domain()) {
            (Some(base), Some(domain)) => domain == base,
            _ => false,
        })
        .map(|mut url| {
            url.set_fragment(None);
            url.to_string()
        })
        .collect()
}

async fn fetch_robots_for_domain(client: &reqwest::Client, domain_base: &Url) -> Vec<String> {
    let robots_url = match domain_base.join("/robots.txt") {
        Ok(u) => u,
        Err(_) => return vec![],
    };

    match client.get(robots_url.as_str()).send().await {
        Ok(resp) => match resp.status() {
            StatusCode::OK => match resp.text().await {
                Ok(text) => parse_robots_txt(&text),
                Err(_) => vec![],
            },
            _ => vec![],
        },
        Err(_) => vec![],
    }
}

fn parse_robots_txt(text: &str) -> Vec<String> {
    let mut disallows = Vec::new();
    let mut applicable = false;
    for raw_line in text.lines() {
        let line = raw_line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.splitn(2, ':').map(|s| s.trim()).collect();
        if parts.len() != 2 {
            continue;
        }
        let (key, value) = (parts[0].to_lowercase(), parts[1]);
        match key.as_str() {
            "user-agent" => {
                applicable = value == "*";
            }
            "disallow" => {
                if applicable && !value.is_empty() {
                    disallows.push(value.to_string());
                }
            }
            "allow" => {
                // TODO: allowed url
            }
            _ => {}
        }
    }
    disallows
}

async fn is_allowed_by_robots(
    state: &Arc<CrawlState>,
    client: &reqwest::Client,
    url: &Url,
) -> bool {
    let domain = match url.domain() {
        Some(d) => d.to_string(),
        None => return true,
    };

    {
        let cache = state.robots_cache.lock().await;
        if let Some(disallows) = cache.get(&domain) {
            for prefix in disallows {
                if url.path().starts_with(prefix) {
                    debug!("robots disallow {} -> {}", url, prefix);
                    return false;
                }
            }
            return true;
        }
    }

    let disallows = fetch_robots_for_domain(client, url).await;
    {
        let mut cache = state.robots_cache.lock().await;
        cache.insert(domain.clone(), disallows.clone());
    }

    for prefix in disallows {
        if url.path().starts_with(&prefix) {
            debug!("robots disallow {} -> {}", url, prefix);
            return false;
        }
    }
    true
}

async fn wait_for_politeness(state: &Arc<CrawlState>, domain: &str, delay_ms: u64) {
    let mut map = state.domain_next_allowed.lock().await;
    let now = Instant::now();
    if let Some(next) = map.get(domain) {
        if *next > now {
            let sleep_dur = *next - now;
            tokio::time::sleep(sleep_dur).await;
        }
    }
    map.insert(
        domain.to_string(),
        Instant::now() + Duration::from_millis(delay_ms),
    );
}

async fn process_url(
    client: reqwest::Client,
    url: String,
    depth: usize,
    state: Arc<CrawlState>,
    max_depth: usize,
    max_pages: usize,
    delay_ms: u64,
    obey_robots: bool,
) {
    let parsed = match Url::parse(&url) {
        Ok(u) => u,
        Err(e) => {
            warn!("Skipping invalid URL {}: {}", url, e);
            let mut stats = state.stats.lock().await;
            stats.failed_requests += 1;
            return;
        }
    };

    if obey_robots {
        if !is_allowed_by_robots(&state, &client, &parsed).await {
            info!("Blocked by robots.txt: {}", url);
            let mut stats = state.stats.lock().await;
            stats.failed_requests += 1;
            return;
        }
    }

    if let Some(domain) = parsed.domain() {
        wait_for_politeness(&state, domain, delay_ms).await;
    }

    info!("Fetching {} (depth={})", url, depth);
    match client.get(&url).send().await {
        Ok(resp) => {
            let status = resp.status();
            if !status.is_success() {
                warn!("Non-success status {} for {}", status, url);
                let mut stats = state.stats.lock().await;
                stats.failed_requests += 1;
                return;
            }
            match resp.text().await {
                Ok(body) => {
                    let new_urls = find_all_urls(&url, &body);
                    let new_count = new_urls.len();

                    {
                        let mut stats = state.stats.lock().await;
                        stats.successful_requests += 1;
                        stats.total_urls_found += new_count as u64;
                    }

                    if depth + 1 <= max_depth {
                        let mut q = state.to_visit.lock().await;
                        let visited_read = state.visited.read().await;
                        for u in new_urls {
                            if visited_read.contains(&u) {
                                continue;
                            }
                            if visited_read.len() + q.len() >= max_pages {
                                break;
                            }
                            q.push_back((u, depth + 1));
                        }
                    }
                }
                Err(e) => {
                    warn!("Failed to read body {}: {}", url, e);
                    let mut stats = state.stats.lock().await;
                    stats.failed_requests += 1;
                }
            }
        }
        Err(e) => {
            warn!("Request error for {}: {}", url, e);
            let mut stats = state.stats.lock().await;
            stats.failed_requests += 1;
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let subscriber = FmtSubscriber::builder().with_env_filter(filter).finish();
    tracing::subscriber::set_global_default(subscriber).expect("Setting default subscriber failed");

    let cli = cli::Cli::parse();
    let seed = cli.url.clone();

    let seed_url = Url::parse(&seed)?;
    if !seed_url.scheme().starts_with("http") {
        return Err("Seed URL must be http or https".into());
    }

    let state = Arc::new(CrawlState::new(seed.clone()));

    let client = reqwest::Client::builder()
        .user_agent("ProbeBot/1.0 (+https://example.com/bot)")
        .timeout(Duration::from_secs(5))
        .build()?;

    let semaphore = Arc::new(Semaphore::new(cli.concurrency));

    info!(
        "Starting crawl: {} | max_pages={} | concurrency={} | max_depth={} | delay_ms={} | obey_robots={}",
        seed, cli.max_pages, cli.concurrency, cli.max_depth, cli.delay_ms, cli.obey_robots
    );

    loop {
        let visited_count = { state.visited.read().await.len() };
        if visited_count >= cli.max_pages {
            info!("Reached max_pages limit: {}", visited_count);
            break;
        }

        let next_job = {
            let mut q = state.to_visit.lock().await;
            q.pop_front()
        };

        let job = match next_job {
            Some((url, depth)) => (url, depth),
            None => {
                if Arc::strong_count(&semaphore) == 1 {
                    break;
                } else {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    continue;
                }
            }
        };

        {
            let visited_read = state.visited.read().await;
            if visited_read.contains(&job.0) {
                continue;
            }
            if visited_read.len() >= cli.max_pages || job.1 > cli.max_depth {
                continue;
            }
        }

        {
            let mut visited_write = state.visited.write().await;
            if !visited_write.insert(job.0.clone()) {
                continue;
            }
        }

        let permit = semaphore.clone().acquire_owned().await.unwrap();
        let client_cl = client.clone();
        let state_cl = Arc::clone(&state);
        let url_cl = job.0.clone();
        let depth_cl = job.1;
        let max_depth = cli.max_depth;
        let max_pages = cli.max_pages;
        let delay_ms = cli.delay_ms;
        let obey_robots = cli.obey_robots;

        tokio::spawn(async move {
            process_url(
                client_cl,
                url_cl,
                depth_cl,
                state_cl,
                max_depth,
                max_pages,
                delay_ms,
                obey_robots,
            )
            .await;
            drop(permit);
        });
    }

    let _ = semaphore.acquire_many(cli.concurrency as u32).await;

    let stats = state.stats.lock().await;
    let visited_final = state.visited.read().await.len();
    let duration = stats.start_time.elapsed();

    let report = serde_json::json!({
        "seed": seed,
        "duration_seconds": duration.as_secs_f64(),
        "visited_unique_urls": visited_final,
        "successful_requests": stats.successful_requests,
        "failed_requests": stats.failed_requests,
        "total_urls_found": stats.total_urls_found,
    });

    let report_str = serde_json::to_string_pretty(&report)?;
    tokio::fs::write(&cli.report, report_str).await?;

    info!(
        "Crawl finished. Visited {} urls in {:.2?}. Report written to {}",
        visited_final, duration, cli.report
    );

    Ok(())
}
