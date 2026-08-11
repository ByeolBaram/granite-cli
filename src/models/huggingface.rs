//! Parsing helpers for the two shapes `ModelVariant.url` takes for
//! HuggingFace-hosted variants: a repo URL (safetensors/MLX variants, e.g.
//! `https://huggingface.co/{owner}/{repo}`) or a full blob URL (GGUF
//! variants, e.g.
//! `https://huggingface.co/{owner}/{repo}/blob/{branch}/{filename}`). A bare
//! `"owner/repo"` id is also accepted for robustness, though generated data
//! always uses a full URL.

/// Extract `"owner/repo"` from a HF repo URL, a HF blob URL, or a bare repo
/// id. Returns `None` if `url` doesn't look like a HuggingFace reference
/// (e.g. an `ollama.com` library URL).
pub fn hf_repo_id(url: &str) -> Option<&str> {
    let rest = if url.starts_with("http") {
        url.strip_prefix("https://huggingface.co/")?
    } else {
        url
    };

    let mut parts = rest.splitn(3, '/');
    let owner = parts.next()?;
    let repo = parts.next()?;
    if owner.is_empty() || repo.is_empty() {
        return None;
    }
    Some(&rest[..owner.len() + 1 + repo.len()])
}

/// Extract the filename from a HF blob URL. Returns `None` for bare repo
/// ids or any URL that isn't a `.../blob/...` reference.
pub fn hf_blob_filename(url: &str) -> Option<&str> {
    if !url.contains("huggingface.co") || !url.contains("/blob/") {
        return None;
    }
    url.rsplit('/').next().filter(|f| !f.is_empty())
}

/*-- tests -------------------------------------------------------------------*/

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hf_repo_id_from_bare_repo() {
        assert_eq!(
            hf_repo_id("ibm-granite/granite-speech-4.1-2b"),
            Some("ibm-granite/granite-speech-4.1-2b")
        );
    }

    #[test]
    fn hf_repo_id_from_repo_url() {
        assert_eq!(
            hf_repo_id("https://huggingface.co/ibm-granite/granite-speech-4.1-2b"),
            Some("ibm-granite/granite-speech-4.1-2b")
        );
    }

    #[test]
    fn hf_repo_id_from_blob_url() {
        assert_eq!(
            hf_repo_id(
                "https://huggingface.co/ibm-granite/granite-speech-4.1-2b-GGUF/blob/main/granite-speech-4.1-2b-Q4_K_M.gguf"
            ),
            Some("ibm-granite/granite-speech-4.1-2b-GGUF")
        );
    }

    #[test]
    fn hf_repo_id_rejects_non_hf_url() {
        assert_eq!(hf_repo_id("https://ollama.com/library/granite4:1b"), None);
    }

    #[test]
    fn hf_blob_filename_from_blob_url() {
        assert_eq!(
            hf_blob_filename(
                "https://huggingface.co/ibm-granite/granite-speech-4.1-2b-GGUF/blob/main/granite-speech-4.1-2b-Q4_K_M.gguf"
            ),
            Some("granite-speech-4.1-2b-Q4_K_M.gguf")
        );
    }

    #[test]
    fn hf_blob_filename_none_for_bare_repo() {
        assert_eq!(hf_blob_filename("ibm-granite/granite-speech-4.1-2b"), None);
    }
}
