use anyhow::{Error as AnyhowError, Result as AnyhowResult};
use log::warn;
use std::collections::HashMap;
use std::fs::read_to_string;
use url::Url;

pub trait ModuleResolver {
    /// Bundle an ES module graph into a single executable string.
    /// If `inline_root_source` is `Some`, the root is side-effect-only inline code.
    /// Implementations may fall back to returning the inline source directly.
    ///
    /// # Errors
    /// Returns an error if bundling fails.
    fn bundle_root(
        &mut self,
        root_spec: &str,
        base_url: &Url,
        inline_root_source: Option<&str>,
    ) -> AnyhowResult<String>;
}

/// A minimal file:// resolver and bundler that concatenates modules in a DFS order
/// and strips import/export syntax for side-effect-only modules.
pub struct SimpleFileModuleResolver {
    /// Cache of loaded module sources by URL.
    cache: HashMap<String, String>,
    /// Track which modules are currently being visited to detect cycles.
    visiting: HashMap<String, bool>,
}

impl SimpleFileModuleResolver {
    #[inline]
    pub fn new() -> Self {
        Self {
            cache: HashMap::new(),
            visiting: HashMap::new(),
        }
    }

    /// Resolve a module specifier relative to a referrer URL.
    #[inline]
    fn resolve_spec(spec: &str, referrer: &Url) -> Option<Url> {
        if let Ok(parsed_url) = Url::parse(spec) {
            return Some(parsed_url);
        }
        referrer.join(spec).ok()
    }

    /// Load text from a URL, caching the result.
    ///
    /// # Errors
    /// Returns an error if the file cannot be read.
    #[inline]
    fn load_text(&mut self, url: &Url) -> AnyhowResult<String> {
        let key = url.as_str().to_owned();
        if let Some(cached) = self.cache.get(&key) {
            return Ok(cached.clone());
        }
        let txt = match url.scheme() {
            "file" => {
                // Simple extension-based MIME/loader guard: only allow .js/.mjs by default.
                let path = url
                    .to_file_path()
                    .map_err(|()| AnyhowError::msg("invalid file url"))?;
                let ext_ok = path
                    .extension()
                    .and_then(|extension| extension.to_str())
                    .is_some_and(|extension| {
                        matches!(extension.to_ascii_lowercase().as_str(), "js" | "mjs")
                    });
                if ext_ok {
                    read_to_string(path)?
                } else {
                    // Keep it non-fatal: return empty source to avoid panics while signaling unsupported types.
                    warn!(
                        "ModuleResolver: skipping unsupported file extension for {}",
                        path.display()
                    );
                    String::new()
                }
            }
            _ => String::new(),
        };
        self.cache.insert(key, txt.clone());
        Ok(txt)
    }

    /// Strip import/export statements from source code.
    /// Extremely simple remover: drop lines starting with import/export.
    /// Adequate for side-effect-only modules in tests.
    #[inline]
    fn strip_import_export(source: &str) -> String {
        let mut out = String::new();
        for line in source.lines() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("import ") || trimmed.starts_with("export ") {
                continue;
            }
            out.push_str(line);
            out.push('\n');
        }
        out
    }

    /// Perform depth-first bundling of a module and its dependencies.
    ///
    /// # Errors
    /// Returns an error if circular imports are detected or loading fails.
    #[inline]
    fn dfs_bundle(&mut self, url: &Url) -> AnyhowResult<String> {
        let key = url.as_str().to_owned();
        if self.visiting.get(&key).copied().unwrap_or(false) {
            return Err(AnyhowError::msg(format!("circular import detected: {key}")));
        }
        self.visiting.insert(key.clone(), true);
        let src = self.load_text(url)?;
        // Naive import resolution: look for lines like import ... from 'spec'; or "spec";
        let mut bundled_deps = String::new();
        for line in src.lines() {
            let trimmed_line = line.trim();
            if !trimmed_line.starts_with("import ") {
                continue;
            }

            let Some(idx) = trimmed_line.rfind('"').or_else(|| trimmed_line.rfind('\'')) else {
                continue;
            };
            let first = &trimmed_line.get(..idx).unwrap_or("");
            let Some(startq) = first.rfind('"').or_else(|| first.rfind('\'')) else {
                continue;
            };
            let spec = first.get(startq.saturating_add(1)..).unwrap_or("");
            if let Some(dep) = Self::resolve_spec(spec, url) {
                bundled_deps.push_str(&self.dfs_bundle(&dep)?);
            }
        }
        let self_code = Self::strip_import_export(&src);
        self.visiting.insert(key, false);
        Ok(format!("{bundled_deps}\n{self_code}"))
    }
}

impl ModuleResolver for SimpleFileModuleResolver {
    #[inline]
    fn bundle_root(
        &mut self,
        root_spec: &str,
        base_url: &Url,
        inline_root_source: Option<&str>,
    ) -> AnyhowResult<String> {
        if let Some(src) = inline_root_source {
            return Ok(Self::strip_import_export(src));
        }
        let root = Self::resolve_spec(root_spec, base_url)
            .ok_or_else(|| AnyhowError::msg(format!("bad specifier: {root_spec}")))?;
        self.dfs_bundle(&root)
    }
}

impl Default for SimpleFileModuleResolver {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}
