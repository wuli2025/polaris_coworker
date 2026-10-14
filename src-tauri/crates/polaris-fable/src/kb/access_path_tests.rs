//! Platform-specific path containment contracts; production logic is unchanged.
use super::{path_contains, Path};

#[test]
#[cfg(not(windows))]
fn path_contains_uses_posix_components_and_case() {
    assert!(path_contains(Path::new("/kb"), Path::new("/kb/x.md")));
    assert!(!path_contains(Path::new("/kb"), Path::new("/kb-backup/x.md")));
    assert!(!path_contains(Path::new("/kb"), Path::new("/KB/x.md")));
    assert!(!path_contains(Path::new("/kb"), Path::new("/outside/x.md")));
}

#[test]
#[cfg(windows)]
fn path_contains_handles_verbatim_prefix_and_case() {
    // 同根, 子在内: 放行
    assert!(path_contains(
        Path::new(r"C:\Users\a\Polaris"),
        Path::new(r"C:\Users\a\Polaris\x.md")
    ));
    // 关键回归: 一端带 Windows `\\?\` 扩展长度前缀、一端没有, 仍判为包含 (旧裸 starts_with 会误判越界)
    assert!(path_contains(
        Path::new(r"C:\Users\a\Polaris"),
        Path::new(r"\\?\C:\Users\a\Polaris\x.md")
    ));
    assert!(path_contains(
        Path::new(r"\\?\C:\Users\a\Polaris"),
        Path::new(r"C:\Users\a\Polaris\x.md")
    ));
    // 伪前缀: 组件级比较不应把 `Polaris-bak` 当成 `Polaris` 的子树
    assert!(!path_contains(
        Path::new(r"C:\Users\a\Polaris"),
        Path::new(r"C:\Users\a\Polaris-bak\x.md")
    ));
    // 真越界: 不在根下
    assert!(!path_contains(
        Path::new(r"C:\Users\a\Polaris"),
        Path::new(r"C:\Windows\System32\drivers\etc\hosts")
    ));
    // Windows 上大小写不敏感: 根与子大小写不一致也应判为包含
    if cfg!(windows) {
        assert!(path_contains(
            Path::new(r"C:\Users\A\Polaris"),
            Path::new(r"c:\users\a\polaris\x.md")
        ));
    }
}

