use clap::Parser;

#[derive(Parser, Debug)]
#[clap(version, about)]
pub struct Cli {
    /// The seed URL to start crawling from
    #[clap(long, short)]
    pub url: String,

    /// Maximum number of pages to visit (unique)
    #[clap(long, short = 'n', default_value = "200")]
    pub max_pages: usize,

    /// Maximum number of concurrent requests
    #[clap(long, short = 'c', default_value = "20")]
    pub concurrency: usize,

    /// Maximum depth from the seed (seed depth = 0)
    #[clap(long, default_value = "5")]
    pub max_depth: usize,

    /// Delay between requests per-domain in milliseconds (politeness)
    #[clap(long, default_value = "250")]
    pub delay_ms: u64,

    /// Respect robots.txt (defaults to true)
    #[clap(long, default_value = "true")]
    pub obey_robots: bool,

    /// Output JSON report file
    #[clap(long, default_value = "crawl_report.json")]
    pub report: String,
}
