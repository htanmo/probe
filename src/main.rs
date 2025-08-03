use std::collections::{HashSet, VecDeque};

use scraper::{Html, Selector};

fn find_all_urls(base_url: &str, html_body: &str) -> HashSet<String> {
    let document = Html::parse_document(&html_body);
    let selector = Selector::parse("a").unwrap();

    let mut set = HashSet::new();

    let a_tags = document.select(&selector);
    for a_tag in a_tags {
        if let Some(link) = a_tag.value().attr("href") {
            let mut link = link.to_owned();
            if link.trim().starts_with("/") {
                link = format!("{}{}", base_url, link);
            }
            set.insert(link);
        }
    }
    set
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let base_url = "https://google.in";

    let limit = 100;
    let mut visited = HashSet::new();
    let mut to_visit = VecDeque::new();

    to_visit.push_back(base_url.to_owned());

    while !to_visit.is_empty() && visited.len() <= limit {
        let current_url = to_visit.pop_front().unwrap();
        if !visited.contains(&current_url) {
            let html_body = reqwest::blocking::get(&current_url)?.text()?;
            let new_urls = find_all_urls(&current_url, &html_body);
            visited.insert(current_url);
            for url in new_urls {
                to_visit.push_back(url);
            }
        }
    }
    println!("Visited: {:#?}", visited);
    println!("To visit: {:#?}", to_visit);
    Ok(())
}
