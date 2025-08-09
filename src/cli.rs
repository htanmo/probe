use clap::{Parser, command};

#[derive(Parser)]
#[command(version, about, long_about = None)]
pub struct Cli {
    /// Starting URL for the crawler.
    #[arg(short, long, required = true, value_name = "URL")]
    pub url: String,
    /// Number of parallel requests to run at once.
    #[arg(short, long, default_value_t = 10, value_name = "NUMBER")]
    pub concurrency: usize,
    /// Maximum Depth to crawl
    #[arg(short = 'd', long, default_value_t = 3, value_name = "DEPTH")]
    pub max_depth: usize,
    /// Maximum total number of pages to visit.
    #[arg(short = 'n', long, default_value_t = 100, value_name = "PAGES")]
    pub max_pages: usize,
}
