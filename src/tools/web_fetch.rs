use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// A tool that allows agents to fetch web content.
pub struct WebFetchTool {
    client: Client,
    domain_allowlist: Vec<String>,
    timeout: Duration,
}

/// Result of a web fetch operation.
#[derive(Debug, Serialize, Deserialize)]
pub struct WebFetchResult {
    pub url: String,
    pub status: u16,
    pub content_type: Option<String>,
    pub body: String,
    pub truncated: bool,
}

impl WebFetchTool {
    /// Create a new web fetch tool with domain allowlist.
    pub fn new(domain_allowlist: Vec<String>) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .user_agent("niki/0.3.0")
            .build()
            .expect("Failed to create HTTP client");

        Self {
            client,
            domain_allowlist,
            timeout: Duration::from_secs(30),
        }
    }

    /// Check if a URL is allowed by the domain allowlist.
    fn is_allowed(&self, url: &str) -> bool {
        if self.domain_allowlist.is_empty() {
            return false; // Empty allowlist = block all
        }

        let domain = url
            .split("://")
            .nth(1)
            .and_then(|d| d.split('/').next())
            .unwrap_or("");

        self.domain_allowlist
            .iter()
            .any(|allowed| domain == allowed || domain.ends_with(&format!(".{allowed}")))
    }

    /// Fetch content from a URL.
    pub async fn fetch(&self, url: &str) -> Result<WebFetchResult> {
        if !self.is_allowed(url) {
            return Err(anyhow::anyhow!(
                "URL '{}' not in domain allowlist: {:?}",
                url,
                self.domain_allowlist
            ));
        }

        let response = self
            .client
            .get(url)
            .send()
            .await
            .with_context(|| format!("Failed to fetch {url}"))?;

        let status = response.status().as_u16();
        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        let body = response
            .text()
            .await
            .with_context(|| format!("Failed to read response body from {url}"))?;

        // Truncate to prevent prompt injection
        let max_chars = 50_000;
        let truncated = body.len() > max_chars;
        let body = if truncated {
            body[..max_chars].to_string()
        } else {
            body
        };

        Ok(WebFetchResult {
            url: url.to_string(),
            status,
            content_type,
            body,
            truncated,
        })
    }
}

/// Format a web fetch result for injection into agent prompts.
pub fn format_for_prompt(result: &WebFetchResult) -> String {
    let mut output = format!("### Web Fetch: {}\n", result.url);
    output.push_str(&format!("Status: {}\n", result.status));
    if let Some(ct) = &result.content_type {
        output.push_str(&format!("Content-Type: {ct}\n"));
    }
    if result.truncated {
        output.push_str("⚠️ Content truncated to 50K chars to prevent injection.\n");
    }
    output.push_str("\n```\n");
    output.push_str(&result.body);
    output.push_str("\n```\n");
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_domain_allowlist_empty_blocks_all() {
        let tool = WebFetchTool::new(vec![]);
        assert!(!tool.is_allowed("https://example.com"));
    }

    #[test]
    fn test_domain_allowlist_exact_match() {
        let tool = WebFetchTool::new(vec!["example.com".to_string()]);
        assert!(tool.is_allowed("https://example.com"));
        assert!(tool.is_allowed("https://example.com/path"));
        assert!(!tool.is_allowed("https://other.com"));
    }

    #[test]
    fn test_domain_allowlist_subdomain() {
        let tool = WebFetchTool::new(vec!["github.com".to_string()]);
        assert!(tool.is_allowed("https://github.com/foo"));
        assert!(!tool.is_allowed("https://raw.githubusercontent.com/foo"));
        assert!(!tool.is_allowed("https://evil-github.com"));
    }
}
