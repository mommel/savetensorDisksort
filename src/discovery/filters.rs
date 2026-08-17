//! Extension filters and AI model category detection heuristics.

use super::file_info::Category;
use std::path::Path;

/// Standard file extensions recognized as AI model checkpoints or weights.
pub const MODEL_EXTENSIONS: &[&str] = &["safetensors", "pt", "pth", "ckpt", "bin", "gguf", "onnx"];

/// Check if a filename extension matches the supported model extensions.
pub fn is_model_extension(ext: &str) -> bool {
    let lower = ext.to_lowercase();
    MODEL_EXTENSIONS.iter().any(|&e| e == lower)
}

/// Check if a file path should be excluded based on common temporary or hidden file patterns.
pub fn should_exclude_path<P: AsRef<Path>>(path: P) -> bool {
    let path = path.as_ref();
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

    if name.starts_with('.') {
        return true;
    }
    if name.ends_with(".tmp") || name.ends_with(".partial") || name.ends_with(".crdownload") {
        return true;
    }

    for component in path.components() {
        let comp_str = component.as_os_str().to_string_lossy();
        if comp_str == ".git" || comp_str == "node_modules" || comp_str == "__pycache__" {
            return true;
        }
    }

    false
}

/// Infer the model category based on filename, full path, extension, and file size in bytes.
pub fn detect_category<P: AsRef<Path>>(path: P, size_bytes: u64) -> Category {
    let path_ref = path.as_ref();
    let full_path_lower = path_ref.to_string_lossy().to_lowercase();
    let filename_lower = path_ref
        .file_name()
        .and_then(|n| n.to_str())
        .map(|s| s.to_lowercase())
        .unwrap_or_default();

    // 1. VAE: filename or parent directory mentions "vae"
    if filename_lower.contains("vae")
        || full_path_lower.contains("/vae/")
        || full_path_lower.contains("\\vae\\")
    {
        return Category::Vae;
    }

    // 2. ControlNet: filename or path mentions controlnet / control_
    if full_path_lower.contains("controlnet")
        || full_path_lower.contains("control_v")
        || filename_lower.starts_with("control_")
    {
        return Category::Controlnet;
    }

    // 3. Upscalers: ESRGAN, RealESRGAN, SwinIR, upscale
    if full_path_lower.contains("esrgan")
        || full_path_lower.contains("realesrgan")
        || full_path_lower.contains("swinir")
        || full_path_lower.contains("upscaler")
        || full_path_lower.contains("upscale_models")
    {
        return Category::Upscaler;
    }

    // 4. Embeddings / Textual Inversion
    if full_path_lower.contains("embeddings")
        || full_path_lower.contains("textual_inversion")
        || full_path_lower.contains("embedding")
    {
        return Category::Embedding;
    }

    // 5. LoRA: path contains "lora" or "loras"
    if full_path_lower.contains("/lora/")
        || full_path_lower.contains("\\lora\\")
        || full_path_lower.contains("/loras/")
        || full_path_lower.contains("\\loras\\")
        || filename_lower.contains("lora")
    {
        return Category::Lora;
    }

    // 6. Checkpoints: path contains "checkpoints", "stable-diffusion", "diffusers" or large file (> 1 GB)
    if full_path_lower.contains("checkpoints")
        || full_path_lower.contains("stable-diffusion")
        || full_path_lower.contains("diffusion_models")
        || full_path_lower.contains("unet")
        || size_bytes > 1_073_741_824
    // > 1 GB
    {
        return Category::Checkpoint;
    }

    // 7. LoRA fallback for smaller safetensors (e.g. 50MB - 350MB)
    let ext = path_ref
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_lowercase())
        .unwrap_or_default();

    if (ext == "safetensors" || ext == "pt") && size_bytes > 5_000_000 && size_bytes < 500_000_000 {
        return Category::Lora;
    }

    // 8. Embedding fallback for very small files (< 20MB)
    if (ext == "pt" || ext == "safetensors" || ext == "bin") && size_bytes <= 5_000_000 {
        return Category::Embedding;
    }

    Category::Other
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_filter_extensions() {
        assert!(is_model_extension("safetensors"));
        assert!(is_model_extension("SAFETENSORS"));
        assert!(is_model_extension("pt"));
        assert!(is_model_extension("pth"));
        assert!(is_model_extension("ckpt"));
        assert!(is_model_extension("gguf"));
        assert!(is_model_extension("onnx"));
        assert!(!is_model_extension("txt"));
        assert!(!is_model_extension("json"));
        assert!(!is_model_extension("py"));
    }

    #[test]
    fn test_exclude_path() {
        assert!(should_exclude_path(".git/objects/123"));
        assert!(should_exclude_path("models/test.tmp"));
        assert!(should_exclude_path("models/file.safetensors.partial"));
        assert!(!should_exclude_path("models/sd/v1-5-pruned.safetensors"));
    }

    #[test]
    fn test_detect_category_heuristics() {
        // VAE
        assert_eq!(
            detect_category(
                "models/vae/vae-ft-mse-840000-ema-pruned.safetensors",
                335_000_000
            ),
            Category::Vae
        );
        assert_eq!(
            detect_category("sd_xl_vae.safetensors", 335_000_000),
            Category::Vae
        );

        // ControlNet
        assert_eq!(
            detect_category(
                "models/controlnet/control_v11p_sd15_canny.pth",
                1_400_000_000
            ),
            Category::Controlnet
        );

        // Upscaler
        assert_eq!(
            detect_category(
                "models/ESRGAN/4x_NMKD-Superscale-SP_178000_G.pth",
                67_000_000
            ),
            Category::Upscaler
        );

        // Embedding
        assert_eq!(
            detect_category("models/embeddings/bad_prompt.pt", 24_000),
            Category::Embedding
        );

        // LoRA
        assert_eq!(
            detect_category("models/loras/detail_tweaker.safetensors", 144_000_000),
            Category::Lora
        );

        // Large Checkpoint
        assert_eq!(
            detect_category("models/v1-5-pruned.safetensors", 4_200_000_000),
            Category::Checkpoint
        );
    }
}
