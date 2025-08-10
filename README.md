# probe

## Build the Project

```sh
cargo build --release
```

## Usage

```sh
Web Crawler

Usage: probe [OPTIONS] --url <URL>

Options:
  -u, --url <URL>                  The seed URL to start crawling from
  -n, --max-pages <MAX_PAGES>      Maximum number of pages to visit (unique) [default: 200]
  -c, --concurrency <CONCURRENCY>  Maximum number of concurrent requests [default: 20]
      --max-depth <MAX_DEPTH>      Maximum depth from the seed (seed depth = 0) [default: 5]
      --delay-ms <DELAY_MS>        Delay between requests per-domain in milliseconds (politeness) [default: 250]
      --obey-robots                Respect robots.txt (defaults to false)
      --report <REPORT>            Output JSON report file [default: report.json]
  -h, --help                       Print help
  -V, --version                    Print version
```
