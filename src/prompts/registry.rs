//! 提示词注册表 — 按 name+version 注册与查询
//!
//! 支持按名称 + 版本查询，未指定版本时返回最新版本。
//!
//! # Examples
//!
//! ```ignore
//! let mut reg = PromptRegistry::new();
//! reg.load_from_dir("prompts")?;
//! let tmpl = reg.get("llm_judge_accuracy", None)?;
//! ```

use std::collections::HashMap;

use crate::error::EvalError;
use crate::prompts::loader;
use crate::prompts::renderer;
use crate::prompts::PromptTemplate;

/// 提示词注册表
///
/// 内部结构：`name → (version → template)`。
/// 支持从目录批量加载，支持按名称 + 可选版本查询。
pub struct PromptRegistry {
    /// name -> (version -> template)
    templates: HashMap<String, HashMap<String, PromptTemplate>>,
}

impl Default for PromptRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl PromptRegistry {
    /// 创建新的提示词注册表
    pub fn new() -> Self {
        Self {
            templates: HashMap::new(),
        }
    }

    /// 从目录加载所有提示词模板（.toml 文件）
    ///
    /// # Arguments
    ///
    /// * `dir` - 提示词文件目录
    ///
    /// # Errors
    ///
    /// 目录不存在、文件解析失败返回错误。
    pub fn load_from_dir(&mut self, dir: &str) -> Result<(), EvalError> {
        let templates = loader::load_prompts_from_dir(dir)?;
        for template in templates {
            self.register(template);
        }
        Ok(())
    }

    /// 注册提示词模板
    ///
    /// 同名同版本将覆盖（打印 warning 日志）。
    pub fn register(&mut self, template: PromptTemplate) {
        let name = template.name.clone();
        let version = template.version.clone();
        tracing::info!(
            name = %name,
            version = %version,
            "注册提示词模板"
        );
        self.templates
            .entry(name)
            .or_default()
            .insert(version, template);
    }

    /// 获取提示词模板
    ///
    /// 若 `version` 为 None，返回最新版本（按字符串排序）。
    ///
    /// # Arguments
    ///
    /// * `name` - 提示词名称
    /// * `version` - 版本号，None 取最新
    ///
    /// # Errors
    ///
    /// 名称不存在返回 [`EvalError::PromptNotFound`]。
    pub fn get(&self, name: &str, version: Option<&str>) -> Result<&PromptTemplate, EvalError> {
        let versions = self
            .templates
            .get(name)
            .ok_or_else(|| EvalError::PromptNotFound(name.to_string()))?;

        let version = match version {
            Some(v) => v,
            None => {
                // 取最新版本（按字符串排序）
                versions
                    .keys()
                    .max()
                    .ok_or_else(|| EvalError::PromptNotFound(name.to_string()))?
            }
        };

        versions
            .get(version)
            .ok_or_else(|| EvalError::PromptNotFound(format!("{name}@{version}")))
    }

    /// 渲染提示词
    ///
    /// # Arguments
    ///
    /// * `template` - 提示词模板
    /// * `context` - tera 渲染上下文
    ///
    /// # Errors
    ///
    /// 渲染失败返回 [`EvalError::PromptRenderError`]。
    pub fn render(&self, template: &PromptTemplate, context: tera::Context) -> Result<String, EvalError> {
        renderer::render(template, context)
    }

    /// 列出所有已注册的提示词名称
    pub fn list_names(&self) -> Vec<String> {
        self.templates.keys().cloned().collect()
    }
}