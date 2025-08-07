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
  -u, --url <URL>             Starting URL for the crawler
  -c, --concurrency <NUMBER>  Number of parallel requests to run at once [default: 10]
  -n, --max-pages <PAGES>     Maximum total number of pages to visit [default: 100]
  -h, --help                  Print help
  -V, --version               Print version
```
