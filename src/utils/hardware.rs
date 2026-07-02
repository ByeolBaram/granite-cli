use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HardwareProfile {
    pub cpu_cores: u32,
    pub cpu_arch: String,
    pub gpu_vendor: Option<String>,
    pub vram_gb: Option<f64>,
    pub ram_gb: f64,
}

impl HardwareProfile {
    pub fn detect() -> Self {
        Self {
            cpu_cores: num_cpus::get() as u32,
            cpu_arch: std::env::consts::ARCH.to_string(),
            gpu_vendor: None,
            vram_gb: None,
            ram_gb: total_memory_bytes() as f64 / (1024.0 * 1024.0 * 1024.0),
        }
    }

    pub fn can_run_model(&self, model_size_gb: f64) -> bool {
        match self.vram_gb {
            Some(vram) => vram >= model_size_gb * 1.5,
            None => self.ram_gb >= model_size_gb * 2.0,
        }
    }

    pub fn recommend_precision(&self) -> &str {
        if let Some(vram) = self.vram_gb {
            if vram >= 24.0 {
                return "BF16";
            }
            if vram >= 12.0 {
                return "Q8_0";
            }
        }
        if self.ram_gb >= 16.0 {
            "Q4_K_M"
        } else {
            "Q3_K_M"
        }
    }
}

pub fn detect_hardware() -> HardwareProfile {
    HardwareProfile::detect()
}

fn total_memory_bytes() -> u64 {
    sys_info::mem_info()
        .map(|info| info.total * 1024)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hardware_profile_detects() {
        let profile = HardwareProfile::detect();
        assert!(profile.cpu_cores > 0);
        assert!(!profile.cpu_arch.is_empty());
        assert!(profile.ram_gb > 0.0);
    }

    #[test]
    fn test_recommend_precision() {
        let profile = HardwareProfile::detect();
        let precision = profile.recommend_precision();
        assert!(matches!(precision, "BF16" | "Q8_0" | "Q4_K_M" | "Q3_K_M"));
    }
}
