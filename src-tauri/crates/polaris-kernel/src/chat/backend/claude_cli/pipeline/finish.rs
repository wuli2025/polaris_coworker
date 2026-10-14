use super::*;

impl TurnStream {
    pub(crate) fn finish(self, art_dir: &Path, art_before: &std::collections::HashMap<std::path::PathBuf, SystemTime>) {
        let Self {
            app,
            req_id,
            conv_id,
            assistant_text,
            mut artifacts,
            ..
        } = self;
        // 产物目录前后快照 diff: 捕获 Bash / 脚本 / Skill 生成的新增或改动文件。
        // 只上报常见成品格式; 打包应用 (含 polaris.project.json 的文件夹) 的内部文件
        // 不逐个上报, 整个应用归并成一个「应用文件夹」chip (路径带尾随 `/`)。
        let art_after = dir_snapshot(art_dir);
        for (path, mtime) in art_after.iter() {
            let changed = match art_before.get(path) {
                None => true,
                Some(old) => mtime > old,
            };
            if !changed {
                continue;
            }
            let s = if let Some(root) = packaged_project_root(path) {
                folder_artifact_repr(&root)
            } else {
                let s = path.to_string_lossy().replace('\\', "/");
                if !is_displayable_artifact(&s) {
                    continue; // 脚本 / 配置 / 临时文件等中间产物: 不进对话框
                }
                s
            };
            if !artifacts.contains(&s) {
                artifacts.push(s.clone());
                emit_event(
                    &app,
                    ChatStreamEvent {
                        req_id: req_id.clone(),
                        kind: "artifact".into(),
                        text: Some(s),
                        tool: None,
                        conversation_id: conv_id.clone(),
                    },
                );
            }
        }

        // 落库前最后一道修剪: 实时阶段上报的文件可能事后被删 / 被归并进应用文件夹,
        // 不让「没有的文件」进历史记录 (重载历史时 chip 全部真实可点)。
        artifacts.retain(|p| {
            if let Some(dir) = p.strip_suffix('/') {
                return Path::new(dir).is_dir();
            }
            let pb = Path::new(p);
            pb.is_file() && is_displayable_artifact(p) && packaged_project_root(pb).is_none()
        });

        // 持久化 assistant 消息 (产物清单以注释 marker 形式存入正文, 重载历史时解析)
        if let Some(cid) = &conv_id {
            let mut content = assistant_text.trim().to_string();
            if !artifacts.is_empty() {
                if let Ok(json) = serde_json::to_string(&artifacts) {
                    content.push_str(&format!("\n\n{}{}-->", ARTIFACT_MARKER_PREFIX, json));
                }
            }
            if !content.trim().is_empty() {
                let _ = conv::append_message(cid, "assistant", &content);
            }
        }

        emit_event(
            &app,
            ChatStreamEvent {
                req_id: req_id.clone(),
                kind: "done".into(),
                text: None,
                tool: None,
                conversation_id: conv_id,
            },
        );
        // 本轮已终态: 清掉可能残留的「取消挂起」标记(如 stop 恰在收尾窗口内到达), 防积攒。
        CHILDREN.take_cancel(&req_id);
    }
}
