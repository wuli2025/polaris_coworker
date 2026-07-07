//! 应用路径的单一来源。
//!
//! 此前 `~/Polaris`、`~/Polaris/data` 等拼接散落在 20+ 个文件里,各自
//! `UserDirs::new()` + `.join("Polaris")`。这里收口成一组命名函数;新代码一律走
//! 这里,旧调用点随模块重构逐步迁入。

use std::path::PathBuf;

/// 用户主目录;取不到时退化为当前目录(与既有各模块的兜底一致)。
pub fn home_dir() -> PathBuf {
    directories::UserDirs::new()
        .map(|u| u.home_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."))
}

/// 应用根目录 `~/Polaris`:用户可见的一切(项目/技能/数据)都在它下面。
pub fn polaris_root() -> PathBuf {
    home_dir().join("Polaris")
}

/// 应用数据目录 `~/Polaris/data`:各模块的 JSON 配置/状态落盘处。
pub fn data_dir() -> PathBuf {
    polaris_root().join("data")
}

/// data 目录下的单个文件,如 `data_file("voice.json")`。
pub fn data_file(name: &str) -> PathBuf {
    data_dir().join(name)
}

/// 产物目录 `~/Polaris/data/artifacts`。
pub fn artifacts_dir() -> PathBuf {
    data_dir().join("artifacts")
}

/// 项目目录 `~/Polaris/projects`。
pub fn projects_dir() -> PathBuf {
    polaris_root().join("projects")
}

/// 技能目录 `~/Polaris/skills`。
pub fn skills_dir() -> PathBuf {
    polaris_root().join("skills")
}

/// 本地模型目录 `~/Polaris/models`。
pub fn models_dir() -> PathBuf {
    polaris_root().join("models")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_is_anchored_under_root() {
        let root = polaris_root();
        assert!(root.ends_with("Polaris"));
        assert!(data_dir().starts_with(&root));
        assert!(artifacts_dir().starts_with(data_dir()));
        assert_eq!(data_file("voice.json"), data_dir().join("voice.json"));
        for d in [projects_dir(), skills_dir(), models_dir()] {
            assert!(d.starts_with(&root));
        }
    }
}
