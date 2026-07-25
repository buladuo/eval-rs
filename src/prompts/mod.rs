//! 提示词管理模块 — 模板化、版本化的提示词管理
//!
//! 提示词以 TOML 文件存放，包含模板内容与变量 schema。
//! 通过 [`registry::PromptRegistry`] 按名称 + 版本查询与渲染。
//!
//! # 模块组成
//!
//! - [`loader`] — 从文件系统加载提示词
//! - [`registry`] — 注册表（name + version → template）
//! - [`renderer`] — 基于 tera 的模板渲染

pub mod loader;
pub mod registry;
pub mod renderer;

use serde::Deserialize;
use std::collections::HashMap;

/// 提示词模板
///
/// 每个模板包含 tera 语法内容与变量 schema。
#[derive(Debug, Clone, Deserialize)]
pub struct PromptTemplate {
    /// 提示词名称（全局唯一标识）
    pub name: String,
    /// 版本号（语义化版本，如 `"1.0"`）
    pub version: String,
    /// 提示词描述
    pub description: String,
    /// 模板内容（tera 模板语法）
    pub template: String,
    /// 变量 schema，描述每个变量的类型与必填信息
    #[serde(default)]
    pub variables: HashMap<String, VariableSchema>,
}

/// 变量 schema
#[derive(Debug, Clone, Deserialize)]
pub struct VariableSchema {
    /// 变量类型: `"string"` | `"number"` | `"boolean"`，默认 `"string"`
    #[serde(default = "default_var_type")]
    pub r#type: String,
    /// 变量说明
    pub description: String,
    /// 是否必填，默认 true
    #[serde(default = "default_required")]
    pub required: bool,
}

fn default_var_type() -> String {
    "string".to_string()
}

fn default_required() -> bool {
    true
}