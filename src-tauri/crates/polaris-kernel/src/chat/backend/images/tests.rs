use super::*;
use std::path::PathBuf;

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> anyhow::Result<Self> {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!("chat-images-{}-{}", std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)));
        fs::create_dir_all(path.join("uploads"))?;
        Ok(Self(path))
    }
    fn uploads(&self) -> PathBuf { self.0.join("uploads") }
    fn image(&self, name: &str, bytes: &[u8]) -> anyhow::Result<String> {
        let path = self.uploads().join(name);
        fs::write(&path, bytes)?;
        Ok(path.to_string_lossy().into())
    }
}
impl Drop for Fixture {
    fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); }
}
fn rejected(f: &Fixture, paths: &[String], limits: &ImageLimits, code: &str) -> anyhow::Result<()> {
    let error = load(&f.uploads(), paths, limits).err()
        .ok_or_else(|| anyhow::anyhow!("expected {code}"))?;
    assert!(error.to_string().starts_with(code), "{error:#}");
    Ok(())
}

#[test]
fn contained_images_use_content_mime_and_base64() -> anyhow::Result<()> {
    let f = Fixture::new()?;
    let limits = ImageLimits::default();
    for (name, bytes, expected) in [
        ("png.bin", &b"\x89PNG\r\n\x1a\nfixture"[..], "image/png"),
        ("jpeg.png", &b"\xff\xd8\xfffixture"[..], "image/jpeg"),
        ("gif", &b"GIF89afixture"[..], "image/gif"),
        ("webp", &b"RIFF1234WEBPfixture"[..], "image/webp"),
    ] {
        let paths = [f.image(name, bytes)?];
        let images = load(&f.uploads(), &paths, &limits)?;
        assert_eq!(images.len(), 1);
        let Attachment::Image { mime_type, data } = &images[0];
        assert_eq!(mime_type, expected);
        assert_eq!(base64::engine::general_purpose::STANDARD.decode(data)?, bytes);
    }
    // No uploads directory is needed by old text-only callers.
    assert!(load(&f.0.join("missing"), &[], &limits)?.is_empty());
    Ok(())
}

#[test]
fn rejects_outside_sibling_traversal_relative_missing_and_directory() -> anyhow::Result<()> {
    let f = Fixture::new()?;
    let limits = ImageLimits::default();
    let inside = f.image("inside.png", b"\x89PNG\r\n\x1a\n")?;
    fs::create_dir_all(f.0.join("uploads-other"))?;
    for outside in [f.0.join("outside.png"), f.0.join("uploads-other/other.png")] {
        fs::write(&outside, b"\x89PNG\r\n\x1a\n")?;
        rejected(&f, &[outside.to_string_lossy().into()], &limits, "ATTACHMENT_PATH_OUTSIDE_UPLOADS")?;
    }
    rejected(&f, &[f.uploads().join("../uploads/inside.png").to_string_lossy().into()],
        &limits, "ATTACHMENT_PATH_INVALID")?;
    rejected(&f, &["inside.png".into()], &limits, "ATTACHMENT_PATH_INVALID")?;
    rejected(&f, &[format!("{inside}.missing")], &limits, "ATTACHMENT_PATH_INVALID")?;
    let directory = f.uploads().join("directory");
    fs::create_dir(&directory)?;
    rejected(&f, &[directory.to_string_lossy().into()], &limits, "ATTACHMENT_NOT_FILE")?;
    Ok(())
}

#[cfg(unix)]
#[test]
fn rejects_symlink_file_directory_and_uploads_escape() -> anyhow::Result<()> {
    use std::os::unix::fs::symlink;
    let f = Fixture::new()?;
    let limits = ImageLimits::default();
    fs::write(f.0.join("outside.png"), b"\x89PNG\r\n\x1a\n")?;
    symlink(f.0.join("outside.png"), f.uploads().join("file.png"))?;
    symlink(&f.0, f.uploads().join("directory"))?;
    for path in ["file.png", "directory/outside.png"] {
        rejected(&f, &[f.uploads().join(path).to_string_lossy().into()],
            &limits, "ATTACHMENT_PATH_OUTSIDE_UPLOADS")?;
    }
    fs::rename(f.uploads(), f.0.join("old-uploads"))?;
    symlink(&f.0, f.uploads())?;
    rejected(&f, &[f.uploads().join("outside.png").to_string_lossy().into()],
        &limits, "ATTACHMENT_PATH_OUTSIDE_UPLOADS")?;
    Ok(())
}

#[test]
fn enforces_count_single_total_and_configured_mime_limits() -> anyhow::Result<()> {
    let f = Fixture::new()?;
    let image = f.image("image", b"\x89PNG\r\n\x1a\n")?;
    let mut limits = ImageLimits::default();
    rejected(&f, &vec![image.clone(); 5], &limits, "ATTACHMENT_COUNT_LIMIT")?;
    limits.max_count = 0;
    rejected(&f, &[image.clone()], &limits, "ATTACHMENT_COUNT_LIMIT")?;
    limits.max_count = 4;
    limits.max_bytes = 7;
    rejected(&f, &[image.clone()], &limits, "ATTACHMENT_SIZE_LIMIT")?;
    limits.max_bytes = 8;
    limits.max_total_bytes = 15;
    rejected(&f, &[image.clone(), image.clone()], &limits, "ATTACHMENT_SIZE_LIMIT")?;
    limits.max_total_bytes = 16;
    assert_eq!(load(&f.uploads(), &[image.clone(), image.clone()], &limits)?.len(), 2);
    limits.mime_types = vec!["image/jpeg".into()];
    rejected(&f, &[image], &limits, "ATTACHMENT_TYPE_UNSUPPORTED")?;
    let fake = f.image("fake.png", b"text")?;
    rejected(&f, &[fake], &ImageLimits::default(), "ATTACHMENT_TYPE_UNSUPPORTED")?;
    Ok(())
}

#[test]
fn configuration_defaults_match_agentd_and_reject_invalid_types() -> anyhow::Result<()> {
    let agentd: serde_json::Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"), "/../../../agentd/config/agent.default.json")))?;
    assert_eq!(serde_json::to_value(ImageLimits::default())?, agentd["attachments"]);
    for json in [r#"{"maxCount":-1}"#, r#"{"maxBytes":"8"}"#,
        r#"{"maxTotalBytes":1.5}"#, r#"{"mimeTypes":null}"#, r#"{"unknown":1}"#] {
        assert!(serde_json::from_str::<ImageLimits>(json).is_err(), "{json}");
    }
    for json in [r#"{"maxBytes":0}"#, r#"{"maxTotalBytes":0}"#, r#"{"mimeTypes":["image/svg+xml"]}"#] {
        let limits: ImageLimits = serde_json::from_str(json)?;
        assert!(limits.validate().is_err(), "{json}");
    }
    let config: super::super::config::Config = serde_json::from_str(
        r#"{"agentd":{"attachments":{"maxCount":1,"maxBytes":8,"maxTotalBytes":8,"mimeTypes":["image/png"]}}}"#)?;
    config.validate()?;
    assert_eq!(config.agentd.attachments.max_count, 1);
    Ok(())
}
