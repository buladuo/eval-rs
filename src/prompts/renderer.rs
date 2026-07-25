//! 提示词渲染器 — 使用 tera 模板引擎渲染提示词
//!
//! 将模板变量注入到提示词模板中，生成最终的提示词文本。
//!
//! # 变量校验
//!
//! 在渲染前校验必填变量是否存在于 context 中，
//! 缺失时返回 [`EvalError::InvalidPromptVariables`]。

use tera::{Context, Tera};

use crate::error::EvalError;
use crate::prompts::PromptTemplate;

/// 渲染提示词模板
///
/// 将 `variables` 注入到 `template` 中，返回渲染后的提示词文本。
///
/// # Arguments
///
/// * `template` - 提示词模板
/// * `context` - tera 渲染上下文
///
/// # Errors
///
/// - 必填变量缺失 → [`EvalError::InvalidPromptVariables`]
/// - 模板语法错误 → [`EvalError::PromptRenderError`]
pub fn render(template: &PromptTemplate, context: Context) -> Result<String, EvalError> {
    // 校验必填变量
    for (name, schema) in &template.variables {
        if schema.required && !context.contains_key(name) {
            return Err(EvalError::InvalidPromptVariables(format!(
                "缺少必填变量: {name}"
            )));
        }
    }

    // 使用 tera 渲染
    let rendered = Tera::one_off(&template.template, &context, false)
        .map_err(|e| EvalError::PromptRenderError(e.to_string()))?;

    Ok(rendered)
}

/// 从 JSON Value 创建 tera Context
///
/// 若 value 是对象，将每个字段作为独立变量注入；
/// 否则将整个 value 作为 `"input"` 变量。
pub fn context_from_json(value: &serde_json::Value) -> Result<Context, EvalError> {
    let mut context = Context::new();

    if let Some(obj) = value.as_object() {
        for (k, v) in obj {
            context.insert(k, v);
        }
    } else {
        context.insert("input", value);
    }

    Ok(context)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn make_template(template: &str, vars: Vec<&str>) -> PromptTemplate {
        let mut variables = std::collections::HashMap::new();
        for v in vars {
            variables.insert(
                v.to_string(),
                crate::prompts::VariableSchema {
                    r#type: "string".to_string(),
                    description: "test".to_string(),
                    required: true,
                },
            );
        }
        PromptTemplate {
            name: "test".to_string(),
            version: "1.0".to_string(),
            description: "test template".to_string(),
            template: template.to_string(),
            variables,
        }
    }

    #[test]
    fn test_render_basic() {
        let template = make_template("Hello, {{ name }}!", vec!["name"]);
        let mut ctx = Context::new();
        ctx.insert("name", &"world");
        let result = render(&template, ctx).unwrap();
        assert_eq!(result, "Hello, world!");
    }

    #[test]
    fn test_render_missing_required() {
        let template = make_template("Hello, {{ name }}!", vec!["name"]);
        let ctx = Context::new();
        let result = render(&template, ctx);
        assert!(matches!(result, Err(EvalError::InvalidPromptVariables(_))));
    }

    #[test]
    fn test_context_from_json() {
        let value = json!({"a": 1, "b": "hello"});
        let ctx = context_from_json(&value).unwrap();
        assert!(ctx.contains_key("a"));
        assert!(ctx.contains_key("b"));
    }
}
