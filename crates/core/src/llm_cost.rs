struct ModelPricing {
    input_per_million: f64,
    output_per_million: f64,
}

pub struct CostResult {
    pub input_cost: f64,
    pub output_cost: f64,
    pub total_cost: f64,
}

fn get_pricing(model: &str) -> Option<ModelPricing> {
    Some(match model {
        "gpt-5.5" => ModelPricing {
            input_per_million: 5.0,
            output_per_million: 30.0,
        },
        "gpt-5.4" => ModelPricing {
            input_per_million: 2.5,
            output_per_million: 15.0,
        },
        "gpt-5.3-codex" => ModelPricing {
            input_per_million: 2.5,
            output_per_million: 10.0,
        },
        "gpt-5.2" => ModelPricing {
            input_per_million: 1.75,
            output_per_million: 14.0,
        },
        "gpt-5.2-codex" => ModelPricing {
            input_per_million: 1.75,
            output_per_million: 7.0,
        },
        "gemini-2.5-pro" => ModelPricing {
            input_per_million: 1.25,
            output_per_million: 10.0,
        },
        "gemini-3-pro-preview" | "gemini-3.1-pro-preview" => ModelPricing {
            input_per_million: 2.0,
            output_per_million: 12.0,
        },
        "deepseek-v4-pro" => ModelPricing {
            input_per_million: 0.55,
            output_per_million: 2.19,
        },
        "MiniMax-M2.7" => ModelPricing {
            input_per_million: 0.30,
            output_per_million: 1.20,
        },
        "claude-fable-5-1" | "claude-fable-5" => ModelPricing {
            input_per_million: 10.0,
            output_per_million: 50.0,
        },
        "claude-opus-5" | "claude-opus-4-7" => ModelPricing {
            input_per_million: 5.0,
            output_per_million: 25.0,
        },
        "grok-4.3" => ModelPricing {
            input_per_million: 1.25,
            output_per_million: 2.50,
        },
        "openrouter/xiaomi/mimo-v2.5-pro" => ModelPricing {
            input_per_million: 0.435,
            output_per_million: 0.87,
        },
        "openrouter/z-ai/glm-5.2" => ModelPricing {
            input_per_million: 1.0,
            output_per_million: 4.0,
        },
        _ => return None,
    })
}

pub fn calculate_cost(prompt_tokens: u64, completion_tokens: u64, model: &str) -> CostResult {
    match get_pricing(model) {
        Some(p) => {
            let input_cost = (prompt_tokens as f64 / 1_000_000.0) * p.input_per_million;
            let output_cost = (completion_tokens as f64 / 1_000_000.0) * p.output_per_million;
            CostResult {
                input_cost,
                output_cost,
                total_cost: input_cost + output_cost,
            }
        }
        None => CostResult {
            input_cost: 0.0,
            output_cost: 0.0,
            total_cost: 0.0,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prices_claude_fable_5_1() {
        let cost = calculate_cost(1_000_000, 1_000_000, "claude-fable-5-1");
        assert!((cost.input_cost - 10.0).abs() < f64::EPSILON);
        assert!((cost.output_cost - 50.0).abs() < f64::EPSILON);
        assert!((cost.total_cost - 60.0).abs() < f64::EPSILON);
    }

    #[test]
    fn prices_claude_fable_5() {
        let cost = calculate_cost(1_000_000, 1_000_000, "claude-fable-5");
        assert!((cost.input_cost - 10.0).abs() < f64::EPSILON);
        assert!((cost.output_cost - 50.0).abs() < f64::EPSILON);
        assert!((cost.total_cost - 60.0).abs() < f64::EPSILON);
    }

    #[test]
    fn prices_claude_opus_5() {
        let cost = calculate_cost(1_000_000, 1_000_000, "claude-opus-5");
        assert!((cost.input_cost - 5.0).abs() < f64::EPSILON);
        assert!((cost.output_cost - 25.0).abs() < f64::EPSILON);
        assert!((cost.total_cost - 30.0).abs() < f64::EPSILON);
    }

    #[test]
    fn prices_openrouter_glm() {
        let cost = calculate_cost(2_530, 2_510, "openrouter/z-ai/glm-5.2");
        assert!((cost.input_cost - 0.00253).abs() < f64::EPSILON);
        assert!((cost.output_cost - 0.01004).abs() < f64::EPSILON);
        assert!((cost.total_cost - 0.01257).abs() < f64::EPSILON);
    }

    #[test]
    fn prices_openrouter_mimo() {
        let cost = calculate_cost(1_000_000, 1_000_000, "openrouter/xiaomi/mimo-v2.5-pro");
        assert!((cost.total_cost - 1.305).abs() < f64::EPSILON);
    }
}
