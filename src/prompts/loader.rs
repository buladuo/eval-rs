//! 提示词加载器 — 从文件系统加载提示词模板
//!
//! 支持 TOML 格式的提示词文件，每个文件定义一个提示词模板。

use std::fs;
use std::path::Path;

use crate::error::EvalError;
use crate::prompts::PromptTemplate;

/// 从指定目录加载所有提示词模板
///
/// 扫描目录下所有 `.toml` 文件，解析为 `PromptTemplate`。
pub fn load_prompts_from_dir(dir: &str) -> Result<Vec<PromptTemplate>, EvalError> {
    let path = Path::new(dir);
    if !path.exists() {
        tracing::warn!(dir = %dir, "提示词目录不存在");
        return Ok(Vec::new());
    }

    let mut templates = Vec::new();

    let entries = fs::read_dir(path)
        .map_err(|e| EvalError::Internal(format!("读取提示词目录失败: {e}")))?;

    for entry in entries {
        let entry = entry.map_err(|e| EvalError::Internal(format!("读取目录项失败: {e}")))?;
        let path = entry.path();

        if path.extension().and_then(|e| e.to_str()) == Some("toml") {
            let content = fs::read_to_string(&path)
                .map_err(|e| EvalError::Internal(format!("读取文件 {:?} 失败: {e}", path)))?;

            let template: PromptTemplate = toml::from_str(&content)
                .map_err(|e| EvalError::Internal(format!("解析提示词 {:?} 失败: {e}", path)))?;

            tracing::info!(
                name = %template.name,
                version = %template.version,
                "加载提示词模板"
            );
            templates.push(template);
        }
    }

    Ok(templates)
}