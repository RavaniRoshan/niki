use anyhow::Result;
use clap::{Args, Subcommand};
use reqwest::Client;
use std::time::Duration;

#[derive(Args)]
pub struct ResearchArgs {
    #[command(subcommand)]
    command: ResearchCommands,
}

#[derive(Subcommand)]
enum ResearchCommands {
    /// Search the web and return a cited summary
    Query {
        /// Search query
        query: String,
        /// Maximum results to fetch
        #[arg(short, long, default_value_t = 5)]
        max_results: usize,
    },
}

pub async fn handle(args: &ResearchArgs) -> Result<()> {
    match &args.command {
        ResearchCommands::Query { query, max_results } => handle_query(query, *max_results).await,
    }
}

async fn handle_query(query: &str, max_results: usize) -> Result<()> {
    let client = Client::builder()
        .timeout(Duration::from_secs(30))
        .user_agent("niki/0.3.0")
        .build()?;

    // Use DuckDuckGo HTML endpoint for search (no API key required)
    let search_url = format!(
        "https://html.duckduckgo.com/html/?q={}",
        urlencoding::encode(query)
    );

    let response = client
        .get(&search_url)
        .header("Accept", "text/html")
        .send()
        .await?;

    let html = response.text().await?;

    // Extract result links and snippets (simple regex-based extraction)
    let results = extract_search_results(&html, max_results);

    if results.is_empty() {
        println!("No results found for: {}", query);
        return Ok(());
    }

    println!("Research: {}\n", query);
    println!("{}", "─".repeat(60));

    for (i, result) in results.iter().enumerate() {
        println!("\n{}. {}", i + 1, result.title);
        println!("   URL: {}", result.url);
        println!("   {}", result.snippet);
    }

    println!("\n{}", "─".repeat(60));
    println!("\nTip: Use `niki chat` with a provider to get a cited summary of these results.");

    Ok(())
}

#[derive(Debug)]
struct SearchResult {
    title: String,
    url: String,
    snippet: String,
}

fn extract_search_results(html: &str, max: usize) -> Vec<SearchResult> {
    let mut results = Vec::new();

    // Simple extraction of result snippets from DuckDuckGo HTML
    for cap in regex::Regex::new(r#"<a[^>]+class="result__a"[^>]*href="([^"]+)"[^>]*>([^<]+)</a>"#)
        .unwrap()
        .captures_iter(html)
    {
        if results.len() >= max {
            break;
        }
        let url = cap[1].to_string();
        let title = cap[2].trim().to_string();

        // Try to extract snippet
        let snippet_re =
            regex::Regex::new(r#"<a[^>]+class="result__snippet"[^>]*>([^<]+)</a>"#).unwrap();
        let snippet = snippet_re
            .captures(html)
            .map(|m| m[1].trim().to_string())
            .unwrap_or_default();

        results.push(SearchResult {
            title,
            url,
            snippet,
        });
    }

    results
}
