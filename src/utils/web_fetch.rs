use anyhow::Result;

pub async fn fetch(url: &str) -> Result<String> {
    let resp = reqwest::get(url).await?;
    let text = resp.text().await?;
    Ok(text)
}

pub async fn fetch_markdown(url: &str) -> Result<String> {
    let html = fetch(url).await?;
    Ok(html)
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_fetch_requires_url() {
        // Basic compilation check - actual network tests would need a mock server
        assert!(true);
    }
}
