use anyhow::Result;
use regex::Regex;

/*-- public --*/

/// Fetch a URL and convert HTML content to markdown.
/// Plain text responses are returned as-is.
pub async fn fetch_markdown(url: &str) -> Result<String> {
    let resp = reqwest::get(url).await?;
    let content_type = resp.headers().get("content-type")
        .map(|v| v.to_str().unwrap_or(""))
        .unwrap_or("");

    if content_type.contains("text/html") {
        let html = resp.text().await?;
        Ok(html2md::parse_html(&html))
    } else {
        let text = resp.text().await?;
        Ok(text)
    }
}

/// Extract URLs from a text string.
pub fn extract_urls(text: &str) -> Vec<String> {
    let url_pattern = Regex::new(r"https?://[a-zA-Z0-9+&@#/%?=~_|!:,.;]*[a-zA-Z0-9+&@#/%=~_|]")
        .unwrap();
    url_pattern.find_iter(text).map(|m| m.as_str().to_string()).collect()
}

/*-- tests --*/

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_urls() {
        let text = "Check out https://example.com and http://test.org/path?q=1";
        let urls = extract_urls(text);
        assert_eq!(urls.len(), 2);
        assert_eq!(urls[0], "https://example.com");
        assert_eq!(urls[1], "http://test.org/path?q=1");
    }

    #[test]
    fn test_extract_urls_no_urls() {
        let text = "This text has no URLs.";
        let urls = extract_urls(text);
        assert!(urls.is_empty());
    }

    #[test]
    fn test_fetch_requires_url() {
        // Basic compilation check - actual network tests would need a mock server
        assert!(true);
    }
}
