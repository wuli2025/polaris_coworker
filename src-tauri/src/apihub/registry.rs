#[cfg(not(feature = "desktop"))]
const REGISTRY_MANIFEST_ACCEPT: &str = "application/vnd.oci.image.index.v1+json,application/vnd.docker.distribution.manifest.list.v2+json,application/vnd.oci.image.manifest.v1+json,application/vnd.docker.distribution.manifest.v2+json";

#[cfg(not(feature = "desktop"))]
#[derive(Debug, Clone)]
struct RegistryTarget {
    revision: String,
    version: String,
    index_digest: String,
    image: String,
}

#[cfg(not(feature = "desktop"))]
fn image_repo_and_tag() -> (String, String) {
    (
        nonempty_env("POLARIS_IMAGE_REPO").unwrap_or_else(|| DEFAULT_IMAGE_REPO.into()),
        nonempty_env("POLARIS_TAG").unwrap_or_else(|| "latest".into()),
    )
}

#[cfg(not(feature = "desktop"))]
fn insecure_registry_host_is_local(base: &str) -> bool {
    let Some(authority) = base.strip_prefix("http://") else {
        return false;
    };
    let authority = authority.split('/').next().unwrap_or("");
    if authority.is_empty() || authority.contains('@') {
        return false;
    }
    let host = if let Some(rest) = authority.strip_prefix('[') {
        let Some((host, suffix)) = rest.split_once(']') else {
            return false;
        };
        if !suffix.is_empty() && !suffix.starts_with(':') {
            return false;
        }
        host
    } else {
        authority
            .split_once(':')
            .map(|(host, _)| host)
            .unwrap_or(authority)
    };
    host.eq_ignore_ascii_case("localhost")
        || host == "127.0.0.1"
        || host == "::1"
        || host.eq_ignore_ascii_case("registry")
}

#[cfg(not(feature = "desktop"))]
fn registry_endpoint(repo: &str) -> Result<(String, String, bool), String> {
    let Some((host, name)) = repo.split_once('/') else {
        return Err("POLARIS_IMAGE_REPO 必须是 registry/namespace/image".into());
    };
    if let Some(base) = nonempty_env("POLARIS_REGISTRY_API_URL") {
        let lower = base.to_ascii_lowercase();
        let insecure = lower.starts_with("http://");
        if !insecure && !lower.starts_with("https://") {
            return Err("POLARIS_REGISTRY_API_URL 只允许 HTTP(S) registry API".into());
        }
        let explicitly_allowed = std::env::var("POLARIS_ALLOW_INSECURE_REGISTRY")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false);
        let local = insecure_registry_host_is_local(&lower);
        if insecure && !(explicitly_allowed || local) {
            return Err("自定义 registry API 使用明文 HTTP；仅测试环境可设置 POLARIS_ALLOW_INSECURE_REGISTRY=1".into());
        }
        return Ok((
            base.trim_end_matches('/').to_string(),
            name.to_string(),
            false,
        ));
    }
    if host != "ghcr.io" {
        return Err(
            "自定义镜像仓库需同时设置 POLARIS_REGISTRY_API_URL，才能验证目标 digest".into(),
        );
    }
    Ok(("https://ghcr.io".into(), name.to_string(), true))
}

#[cfg(not(feature = "desktop"))]
fn registry_agent() -> ureq::Agent {
    let mut builder = ureq::AgentBuilder::new()
        // ghcr 的 blob 会 302 到 GitHub CDN；Docker 默认网络没有 IPv6 路由，
        // AAAA 地址只会报 os error 99，所以先试 IPv4。
        .resolver(|netloc: &str| {
            use std::net::ToSocketAddrs;
            let mut addrs: Vec<_> = netloc.to_socket_addrs()?.collect();
            addrs.sort_by_key(|a| a.is_ipv6());
            Ok(addrs)
        });
    // 国内 NAS 常靠 HTTPS_PROXY 才能访问 GitHub CDN；与 agentd 一致读取代理环境变量。
    if let Some(proxy) = registry_proxy_from_env() {
        builder = builder.proxy(proxy);
    }
    builder
        .timeout_connect(std::time::Duration::from_secs(8))
        .timeout_read(std::time::Duration::from_secs(25))
        .timeout_write(std::time::Duration::from_secs(10))
        .build()
}

/// 只给默认的 ghcr 路径用代理；自定义 registry（含本地测试 registry）保持直连。
#[cfg(not(feature = "desktop"))]
fn registry_proxy_from_env() -> Option<ureq::Proxy> {
    if nonempty_env("POLARIS_REGISTRY_API_URL").is_some() {
        return None;
    }
    ["HTTPS_PROXY", "https_proxy", "ALL_PROXY", "all_proxy"]
        .iter()
        .find_map(|k| nonempty_env(k))
        .and_then(|url| ureq::Proxy::new(url).ok())
}

#[cfg(not(feature = "desktop"))]
fn registry_request(
    method: &str,
    url: &str,
    bearer: Option<&str>,
    accept: Option<&str>,
) -> ureq::Request {
    let mut req = registry_agent().request(method, url);
    if let Some(token) = bearer {
        req = req.set("Authorization", &format!("Bearer {token}"));
    }
    if let Some(value) = accept {
        req = req.set("Accept", value);
    }
    req
}

#[cfg(not(feature = "desktop"))]
fn select_platform_manifest(index: &Value, arch: &str) -> Result<String, String> {
    index
        .get("manifests")
        .and_then(Value::as_array)
        .and_then(|items| {
            items.iter().find(|item| {
                item.pointer("/platform/os").and_then(Value::as_str) == Some("linux")
                    && item
                        .pointer("/platform/architecture")
                        .and_then(Value::as_str)
                        == Some(arch)
            })
        })
        .and_then(|item| item.get("digest"))
        .and_then(Value::as_str)
        .filter(|digest| !digest.is_empty())
        .map(str::to_string)
        .ok_or_else(|| format!("镜像索引没有 linux/{arch} manifest"))
}

#[cfg(not(feature = "desktop"))]
fn image_build_labels(config: &Value) -> Result<(String, String), String> {
    let labels = config.pointer("/config/Labels").unwrap_or(&Value::Null);
    let revision = labels
        .get("org.opencontainers.image.revision")
        .and_then(Value::as_str)
        .filter(|v| !v.is_empty())
        .ok_or("镜像缺少 org.opencontainers.image.revision，无法证明更新目标")?
        .to_string();
    let version = labels
        .get("io.polaris.app.version")
        .and_then(Value::as_str)
        .filter(|v| !v.is_empty())
        .unwrap_or("unknown")
        .to_string();
    Ok((revision, version))
}

#[cfg(not(feature = "desktop"))]
fn docker_registry_target() -> Result<RegistryTarget, String> {
    let (repo, tag) = image_repo_and_tag();
    let (api, name, ghcr) = registry_endpoint(&repo)?;
    let token = if ghcr {
        let resp = registry_agent()
            .get("https://ghcr.io/token")
            .query("service", "ghcr.io")
            .query("scope", &format!("repository:{name}:pull"))
            .call()
            .map_err(|e| format!("GHCR 匿名 pull token 获取失败:{e}"))?;
        let value: Value = resp
            .into_json()
            .map_err(|e| format!("GHCR token 响应无效:{e}"))?;
        Some(
            value
                .get("token")
                .and_then(Value::as_str)
                .filter(|v| !v.is_empty())
                .ok_or("GHCR 没返回匿名 pull token")?
                .to_string(),
        )
    } else {
        None
    };
    let auth = token.as_deref();
    let index_url = format!("{api}/v2/{name}/manifests/{tag}");
    let index_resp = registry_request(
        "GET",
        &index_url,
        auth,
        Some(REGISTRY_MANIFEST_ACCEPT),
    )
    .call()
    .map_err(|e| format!("读取镜像索引失败:{e}"))?;
    let index_digest = index_resp
        .header("Docker-Content-Digest")
        .filter(|value| !value.is_empty())
        .ok_or("registry 未返回 Docker-Content-Digest，无法钉住更新目标")?
        .to_string();
    let index: Value = index_resp
        .into_json()
        .map_err(|e| format!("镜像索引不是 JSON:{e}"))?;
    let arch = match std::env::consts::ARCH {
        "x86_64" => "amd64",
        "aarch64" => "arm64",
        other => other,
    };
    let manifest: Value = if index.get("manifests").is_some() {
        let manifest_digest = select_platform_manifest(&index, arch)?;
        let manifest_url = format!("{api}/v2/{name}/manifests/{manifest_digest}");
        registry_request(
            "GET",
            &manifest_url,
            auth,
            Some(REGISTRY_MANIFEST_ACCEPT),
        )
        .call()
        .map_err(|e| format!("读取 {arch} manifest 失败:{e}"))?
        .into_json()
        .map_err(|e| format!("架构 manifest 不是 JSON:{e}"))?
    } else if index.pointer("/config/digest").is_some() {
        // 普通 `docker build && push` 可能直接发布单架构 manifest，而不是 index。
        // 当前容器能拉起这个 tag 已经证明架构兼容；仍从 config label 核对目标 revision。
        index
    } else {
        return Err("registry 返回内容既不是 OCI index，也不是 image manifest".into());
    };
    let config_digest = manifest
        .pointer("/config/digest")
        .and_then(Value::as_str)
        .ok_or("架构 manifest 没有 config digest")?;
    let config_url = format!("{api}/v2/{name}/blobs/{config_digest}");
    let config: Value = registry_request("GET", &config_url, auth, None)
        .call()
        .map_err(|e| format!("读取镜像 config 失败:{e}"))?
        .into_json()
        .map_err(|e| format!("镜像 config 不是 JSON:{e}"))?;
    let (revision, version) = image_build_labels(&config)?;
    Ok(RegistryTarget {
        revision,
        version,
        index_digest,
        image: format!("{repo}:{tag}"),
    })
}

