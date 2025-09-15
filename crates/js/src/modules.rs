use std::collections::HashMap;

pub trait ModuleResolver {
    /// Bundle an ES module graph into a single executable string.
    /// If `inline_root_source` is Some, the root is side-effect-only inline code.
    /// Implementations may fall back to returning the inline source directly.
    fn bundle_root(
        &mut self,
        root_spec: &str,
        base_url: &url::Url,
        inline_root_source: Option<&str>,
    ) -> anyhow::Result<String>;
}

/// A minimal file:// resolver and bundler that concatenates modules in a DFS order
/// and strips import/export syntax for side-effect-only modules.
pub struct SimpleFileModuleResolver {
    cache: HashMap<String, String>,
    visiting: HashMap<String, bool>,
}

impl SimpleFileModuleResolver {
    pub fn new() -> Self {
        Self {
            cache: HashMap::new(),
            visiting: HashMap::new(),
        }
    }

    fn resolve_spec(&self, spec: &str, referrer: &url::Url) -> Option<url::Url> {
        if let Ok(u) = url::Url::parse(spec) {
            return Some(u);
        }
        referrer.join(spec).ok()
    }

    fn load_text(&mut self, url: &url::Url) -> anyhow::Result<String> {
        let key = url.as_str().to_string();
        if let Some(cached) = self.cache.get(&key) {
            return Ok(cached.clone());
        }
        let txt = match url.scheme() {
            "file" => {
                // Simple extension-based MIME/loader guard: only allow .js/.mjs by default.
                let path = url
                    .to_file_path()
                    .map_err(|_| anyhow::anyhow!("invalid file url"))?;
                let ext_ok = path
                    .extension()
                    .and_then(|e| e.to_str())
                    .map(|e| matches!(e.to_ascii_lowercase().as_str(), "js" | "mjs"))
                    .unwrap_or(false);
                if !ext_ok {
                    // Keep it non-fatal: return empty source to avoid panics while signaling unsupported types.
                    log::warn!(
                        "ModuleResolver: skipping unsupported file extension for {:?}",
                        path
                    );
                    String::new()
                } else {
                    std::fs::read_to_string(path)?
                }
            }
            _ => String::new(),
        };
        self.cache.insert(key, txt.clone());
        Ok(txt)
    }

    fn strip_import_export(source: &str) -> String {
        // Extremely simple remover: drop lines starting with import/export.
        // Adequate for side-effect-only modules in tests.
        let mut out = String::new();
        for line in source.lines() {
            let t = line.trim_start();
            if t.starts_with("import ") || t.starts_with("export ") {
                continue;
            }
            out.push_str(line);
            out.push('\n');
        }
        out
    }

    fn dfs_bundle(&mut self, url: &url::Url) -> anyhow::Result<String> {
        let key = url.as_str().to_string();
        if self.visiting.get(&key).copied().unwrap_or(false) {
            return Err(anyhow::anyhow!("circular import detected: {}", key));
        }
        self.visiting.insert(key.clone(), true);
        let src = self.load_text(url)?;
        // Naive import resolution: look for lines like import ... from 'spec'; or "spec";
        let mut bundled_deps = String::new();
        for line in src.lines() {
            let s = line.trim();
            if !s.starts_with("import ") {
                continue;
            }

            let Some(idx) = s.rfind('"').or_else(|| s.rfind('\'')) else {
                continue;
            };
            let first = &s[..idx];
            let Some(startq) = first.rfind('"').or_else(|| first.rfind('\'')) else {
                continue;
            };
            let spec = &first[startq + 1..];
            if let Some(dep) = self.resolve_spec(spec, url) {
                bundled_deps.push_str(&self.dfs_bundle(&dep)?);
            }
        }
        let self_code = Self::strip_import_export(&src);
        self.visiting.insert(key, false);
        Ok(format!("{}\n{}", bundled_deps, self_code))
    }
}

impl ModuleResolver for SimpleFileModuleResolver {
    fn bundle_root(
        &mut self,
        root_spec: &str,
        base_url: &url::Url,
        inline_root_source: Option<&str>,
    ) -> anyhow::Result<String> {
        if let Some(src) = inline_root_source {
            return Ok(Self::strip_import_export(src));
        }
        let root = self
            .resolve_spec(root_spec, base_url)
            .ok_or_else(|| anyhow::anyhow!("bad specifier: {}", root_spec))?;
        self.dfs_bundle(&root)
    }
}

impl Default for SimpleFileModuleResolver {
    fn default() -> Self {
        Self::new()
    }
}
