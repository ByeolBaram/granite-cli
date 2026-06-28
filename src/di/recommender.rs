use crate::providers::{ModelFormat, Precision};
use crate::registry::ModelDefinition;
use crate::utils::HardwareProfile;

/// Recommends appropriate model variants based on provider capabilities
/// and the user's hardware profile.
pub struct ModelRecommender;

impl ModelRecommender {
    pub fn new() -> Self {
        Self
    }

    /// Recommend model variants for a given model, filtering by
    /// provider capabilities and hardware constraints.
    pub fn recommend_variant(
        &self,
        model: &ModelDefinition,
        provider_formats: &[ModelFormat],
        provider_precisions: &[Precision],
        hardware: &HardwareProfile,
    ) -> Vec<RankedVariant> {
        let mut candidates: Vec<RankedVariant> = model
            .variants
            .iter()
            .filter(|v| {
                // Check provider format compatibility
                let format_match = provider_formats.is_empty()
                    || provider_formats.iter().any(|pf| {
                        format!("{:?}", pf).to_lowercase() == v.format.to_lowercase()
                    });

                // Check provider precision compatibility
                let precision_match = provider_precisions.is_empty()
                    || provider_precisions.iter().any(|pp| {
                        format!("{:?}", pp).to_lowercase() == v.precision.to_lowercase()
                    });

                format_match && precision_match
            })
            .map(|v| {
                let score = self.rank_variant(v, hardware);
                RankedVariant {
                    format: v.format.clone(),
                    precision: v.precision.clone(),
                    size_gb: v.size_gb,
                    huggingface_path: v.huggingface_path.clone(),
                    score,
                    can_run: hardware.can_run_model(v.size_gb),
                }
            })
            .collect();

        candidates.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        candidates
    }

    /// Rank a single variant by suitability for the given hardware.
    fn rank_variant(&self, variant: &crate::registry::models::ModelVariant, hardware: &HardwareProfile) -> f64 {
        let mut score = 100.0;

        // Penalize variants that exceed hardware capacity
        if !hardware.can_run_model(variant.size_gb) {
            score -= 80.0;
        }

        // Prefer formats that match provider capabilities
        if variant.format.to_lowercase() == "gguf" {
            score += 10.0;
        }

        // Prefer quantized formats for limited hardware
        if hardware.vram_gb.map_or(true, |v| v < 16.0) {
            if variant.precision.to_lowercase() == "q4_k_m" || variant.precision.to_lowercase() == "q5_k_m" {
                score += 15.0;
            } else if variant.precision.to_lowercase() == "q8_0" {
                score += 5.0;
            } else if variant.precision.to_lowercase() == "bf16" || variant.precision.to_lowercase() == "fp16" {
                score -= 10.0;
            }
        }

        score
    }

    /// Get a human-readable explanation for the top recommendation.
    pub fn recommendation_explanation(&self, top: &RankedVariant, hardware: &HardwareProfile) -> String {
        let mut parts = Vec::new();

        if hardware.vram_gb.is_some() {
            parts.push("GPU-accelerated".to_string());
        } else {
            parts.push("CPU-only".to_string());
        }

        if top.can_run {
            parts.push("fits available memory".to_string());
        } else {
            parts.push("exceeds available memory".to_string());
        }

        format!("Recommended: {} ({}) — {}",
            top.precision,
            top.format,
            parts.join(", ")
        )
    }
}

/// A ranked model variant recommendation.
pub struct RankedVariant {
    pub format: String,
    pub precision: String,
    pub size_gb: f64,
    pub huggingface_path: String,
    pub score: f64,
    pub can_run: bool,
}

impl std::fmt::Display for RankedVariant {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} / {} ({} GB) [{}]",
            self.format, self.precision, self.size_gb,
            if self.can_run { "can run" } else { "too large" }
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::Registry;

    #[test]
    fn test_recommender_returns_candidates() {
        let recommender = ModelRecommender::new();
        let model = crate::registry::MODEL_REGISTRY.get("granite-3.1-3b-instruct").unwrap();
        let hardware = HardwareProfile {
            cpu_cores: 8,
            cpu_arch: "x86_64".to_string(),
            gpu_vendor: None,
            vram_gb: None,
            ram_gb: 16.0,
        };

        let formats = [ModelFormat::GGUF];
        let precisions = [Precision::Q4_K_M, Precision::Q8_0];

        let variants = recommender.recommend_variant(
            model,
            &formats,
            &precisions,
            &hardware,
        );

        assert!(!variants.is_empty());
    }

    #[test]
    fn test_recommender_scores_by_hardware() {
        let recommender = ModelRecommender::new();

        // Small hardware — should prefer quantized
        let small_hw = HardwareProfile {
            cpu_cores: 4,
            cpu_arch: "x86_64".to_string(),
            gpu_vendor: None,
            vram_gb: None,
            ram_gb: 8.0,
        };

        let model = crate::registry::MODEL_REGISTRY.get("granite-3.1-3b-instruct").unwrap();

        let formats = [ModelFormat::GGUF];
        let precisions = [Precision::Q4_K_M, Precision::Q8_0];

        let variants = recommender.recommend_variant(
            model,
            &formats,
            &precisions,
            &small_hw,
        );

        // Should have candidates
        assert!(!variants.is_empty());

        // All variants should be smaller than RAM
        for v in &variants {
            assert!(v.size_gb < small_hw.ram_gb);
        }
    }
}
