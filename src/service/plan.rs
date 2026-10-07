use crate::catalog::ModelRegistry;
use crate::group_thread_store::{StoredGroup, is_group_id};
use crate::schema::ModelSelector;

/// Where the result of a run is rendered: a single Response, or a group
/// markdown document persisted to the group thread store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OutputShape {
    Single,
    Group { existing_id: Option<String> },
}

#[derive(Debug)]
pub struct ResumePlan {
    pub threads: Vec<Option<String>>,
    pub matched_entry_indices: Vec<Option<usize>>,
    pub output: OutputShape,
}

pub fn normalize_models(
    registry: &ModelRegistry,
    selector: Option<ModelSelector>,
    group_fallback: Option<&StoredGroup>,
) -> anyhow::Result<Vec<String>> {
    let raw = match selector {
        Some(s) => s.into_vec(),
        None => match group_fallback {
            Some(g) => g.entries.iter().map(|e| e.model.clone()).collect(),
            None if !registry.default_models.is_empty() => registry.default_models.clone(),
            None => vec![registry.resolve_model(None)?],
        },
    };
    if raw.is_empty() {
        anyhow::bail!("`model` array must contain at least one entry");
    }
    if raw.len() > 5 {
        anyhow::bail!(
            "max 5 total runs per call, including duplicates (got {})",
            raw.len()
        );
    }
    let mut resolved: Vec<String> = Vec::with_capacity(raw.len());
    for item in raw {
        resolved.push(registry.resolve_model(Some(&item))?);
    }
    Ok(resolved)
}

pub fn plan_resume(
    thread_id: Option<&str>,
    resolved_models: &[String],
    loaded_group: Option<StoredGroup>,
) -> anyhow::Result<ResumePlan> {
    match (thread_id, loaded_group) {
        (None, _) => Ok(ResumePlan {
            threads: vec![None; resolved_models.len()],
            matched_entry_indices: vec![None; resolved_models.len()],
            output: if resolved_models.len() == 1 {
                OutputShape::Single
            } else {
                OutputShape::Group { existing_id: None }
            },
        }),
        (Some(tid), Some(group)) if is_group_id(tid) => {
            let mut threads = Vec::with_capacity(resolved_models.len());
            let mut matched_entry_indices = Vec::with_capacity(resolved_models.len());
            let mut consumed = vec![false; group.entries.len()];
            for m in resolved_models {
                let Some((idx, entry)) = group
                    .entries
                    .iter()
                    .enumerate()
                    .find(|(idx, entry)| !consumed[*idx] && entry.model == *m)
                else {
                    anyhow::bail!(
                        "group {tid} has no remaining member for model {m}; group entries: {:?}",
                        group.entries.iter().map(|e| &e.model).collect::<Vec<_>>()
                    );
                };
                consumed[idx] = true;
                threads.push(Some(entry.thread_id.clone()));
                matched_entry_indices.push(Some(idx));
            }
            Ok(ResumePlan {
                threads,
                matched_entry_indices,
                output: OutputShape::Group {
                    existing_id: Some(tid.to_string()),
                },
            })
        }
        (Some(tid), _) if is_group_id(tid) => {
            anyhow::bail!("group thread not found: {tid}")
        }
        (Some(tid), _) => {
            if resolved_models.len() > 1 {
                anyhow::bail!(
                    "per-model thread_id cannot be used with multiple models; pass a group thread id or omit thread_id"
                );
            }
            Ok(ResumePlan {
                threads: vec![Some(tid.to_string())],
                matched_entry_indices: vec![None],
                output: OutputShape::Single,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::ModelRegistry;
    use crate::group_thread_store::GroupEntry;

    fn reg() -> ModelRegistry {
        ModelRegistry {
            allowed_models: vec![
                "gpt-5.2".into(),
                "gpt-5.4".into(),
                "gemini-2.5-pro".into(),
                "gemini-3.1-pro-preview".into(),
            ],
            fallback_model: "gpt-5.2".into(),
            default_model: None,
            default_models: vec![],
            selector_models: std::collections::HashMap::new(),
        }
    }

    fn entry(model: &str, thread_id: &str) -> GroupEntry {
        GroupEntry {
            model: model.into(),
            thread_id: thread_id.into(),
        }
    }

    #[test]
    fn normalize_none_with_no_group_uses_default() {
        let out = normalize_models(&reg(), None, None).unwrap();
        assert_eq!(out, vec!["gpt-5.2"]);
    }

    #[test]
    fn normalize_single_string() {
        let out =
            normalize_models(&reg(), Some(ModelSelector::One("gemini".into())), None).unwrap();
        assert_eq!(out, vec!["gemini-3.1-pro-preview"]);
    }

    #[test]
    fn normalize_preserves_duplicates_after_resolution() {
        let out = normalize_models(
            &reg(),
            Some(ModelSelector::Many(vec![
                "gemini".into(),
                "gemini-3.1-pro-preview".into(),
                "openai".into(),
            ])),
            None,
        )
        .unwrap();
        assert_eq!(
            out,
            vec![
                "gemini-3.1-pro-preview",
                "gemini-3.1-pro-preview",
                "gpt-5.4"
            ]
        );
    }

    #[test]
    fn normalize_caps_at_5_including_duplicates() {
        let out = normalize_models(
            &reg(),
            Some(ModelSelector::Many(vec![
                "openai".into(),
                "openai".into(),
                "openai".into(),
                "openai".into(),
                "openai".into(),
                "openai".into(),
            ])),
            None,
        );
        assert!(out.is_err());
        assert!(
            out.unwrap_err()
                .to_string()
                .contains("including duplicates")
        );
    }

    #[test]
    fn normalize_empty_array_errors() {
        let out = normalize_models(&reg(), Some(ModelSelector::Many(vec![])), None);
        assert!(out.is_err());
    }

    #[test]
    fn normalize_falls_back_to_group_entries() {
        let group = StoredGroup {
            id: "group_abc".into(),
            entries: vec![
                entry("gpt-5.2", "api_x"),
                entry("gpt-5.2", "api_y"),
                entry("gemini-2.5-pro", "api_z"),
            ],
        };
        let out = normalize_models(&reg(), None, Some(&group)).unwrap();
        assert_eq!(out, vec!["gpt-5.2", "gpt-5.2", "gemini-2.5-pro"]);
    }

    fn reg_with_default_models(default_models: Vec<&str>) -> ModelRegistry {
        ModelRegistry {
            default_models: default_models.into_iter().map(str::to_string).collect(),
            ..reg()
        }
    }

    #[test]
    fn normalize_none_uses_configured_default_models() {
        let out = normalize_models(
            &reg_with_default_models(vec!["gpt-5.2", "gemini-2.5-pro"]),
            None,
            None,
        )
        .unwrap();
        assert_eq!(out, vec!["gpt-5.2", "gemini-2.5-pro"]);
    }

    #[test]
    fn normalize_none_preserves_duplicate_default_models() {
        let out = normalize_models(
            &reg_with_default_models(vec!["gpt-5.2", "gpt-5.2", "gemini-2.5-pro"]),
            None,
            None,
        )
        .unwrap();
        assert_eq!(out, vec!["gpt-5.2", "gpt-5.2", "gemini-2.5-pro"]);
    }

    #[test]
    fn normalize_none_with_empty_default_models_uses_default_model() {
        let mut registry = reg();
        registry.default_model = Some("gemini-2.5-pro".into());

        let out = normalize_models(&registry, None, None).unwrap();

        assert_eq!(out, vec!["gemini-2.5-pro"]);
    }

    #[test]
    fn normalize_group_fallback_wins_over_configured_default_models() {
        let group = StoredGroup {
            id: "group_abc".into(),
            entries: vec![entry("gpt-5.2", "api_x"), entry("gpt-5.2", "api_y")],
        };

        let out = normalize_models(
            &reg_with_default_models(vec!["gemini-2.5-pro"]),
            None,
            Some(&group),
        )
        .unwrap();

        assert_eq!(out, vec!["gpt-5.2", "gpt-5.2"]);
    }

    #[test]
    fn plan_resume_no_tid_single_is_single_output() {
        let plan = plan_resume(None, &["gpt-5.2".into()], None).unwrap();
        assert_eq!(plan.output, OutputShape::Single);
        assert_eq!(plan.threads, vec![None]);
    }

    #[test]
    fn plan_resume_no_tid_multi_is_group_output() {
        let plan = plan_resume(None, &["gpt-5.2".into(), "gemini-2.5-pro".into()], None).unwrap();
        assert_eq!(plan.output, OutputShape::Group { existing_id: None });
        assert_eq!(plan.threads, vec![None, None]);
    }

    #[test]
    fn plan_resume_per_model_tid_with_multi_errors() {
        let err = plan_resume(
            Some("api_xxx"),
            &["gpt-5.2".into(), "gemini-2.5-pro".into()],
            None,
        )
        .unwrap_err();
        assert!(err.to_string().contains("per-model thread_id"));
    }

    #[test]
    fn plan_resume_per_model_tid_with_duplicate_multi_errors() {
        let err =
            plan_resume(Some("api_xxx"), &["gpt-5.2".into(), "gpt-5.2".into()], None).unwrap_err();
        assert!(err.to_string().contains("per-model thread_id"));
    }

    #[test]
    fn plan_resume_per_model_tid_single() {
        let plan = plan_resume(Some("api_xxx"), &["gpt-5.2".into()], None).unwrap();
        assert_eq!(plan.output, OutputShape::Single);
        assert_eq!(plan.threads, vec![Some("api_xxx".to_string())]);
        assert_eq!(plan.matched_entry_indices, vec![None]);
    }

    #[test]
    fn plan_resume_group_tid_member_subset() {
        let group = StoredGroup {
            id: "group_abc".into(),
            entries: vec![entry("gpt-5.2", "api_x"), entry("gemini-2.5-pro", "api_y")],
        };
        let plan = plan_resume(
            Some("group_abc"),
            &["gpt-5.2".into(), "gemini-2.5-pro".into()],
            Some(group),
        )
        .unwrap();
        assert_eq!(
            plan.output,
            OutputShape::Group {
                existing_id: Some("group_abc".into())
            }
        );
        assert_eq!(
            plan.threads,
            vec![Some("api_x".to_string()), Some("api_y".to_string())]
        );
        assert_eq!(plan.matched_entry_indices, vec![Some(0), Some(1)]);
    }

    #[test]
    fn plan_resume_group_tid_duplicate_greedy() {
        let group = StoredGroup {
            id: "group_abc".into(),
            entries: vec![
                entry("gpt-5.2", "api_1"),
                entry("gemini-2.5-pro", "api_g"),
                entry("gpt-5.2", "api_2"),
            ],
        };
        let plan = plan_resume(
            Some("group_abc"),
            &["gpt-5.2".into(), "gpt-5.2".into()],
            Some(group),
        )
        .unwrap();
        assert_eq!(
            plan.threads,
            vec![Some("api_1".to_string()), Some("api_2".to_string())]
        );
        assert_eq!(plan.matched_entry_indices, vec![Some(0), Some(2)]);
    }

    #[test]
    fn plan_resume_group_tid_duplicate_subset_consumes_first_match() {
        let group = StoredGroup {
            id: "group_abc".into(),
            entries: vec![
                entry("gpt-5.2", "api_1"),
                entry("gpt-5.2", "api_2"),
                entry("gemini-2.5-pro", "api_g"),
            ],
        };
        let plan = plan_resume(
            Some("group_abc"),
            &["gpt-5.2".into(), "gemini-2.5-pro".into()],
            Some(group),
        )
        .unwrap();
        assert_eq!(
            plan.threads,
            vec![Some("api_1".to_string()), Some("api_g".to_string())]
        );
        assert_eq!(plan.matched_entry_indices, vec![Some(0), Some(2)]);
    }

    #[test]
    fn plan_resume_group_tid_single_member_uses_group_output() {
        let group = StoredGroup {
            id: "group_abc".into(),
            entries: vec![entry("gpt-5.2", "api_x"), entry("gemini-2.5-pro", "api_y")],
        };
        let plan = plan_resume(Some("group_abc"), &["gpt-5.2".into()], Some(group)).unwrap();
        assert_eq!(
            plan.output,
            OutputShape::Group {
                existing_id: Some("group_abc".into())
            },
            "group resumes always use the group path"
        );
    }

    #[test]
    fn plan_resume_group_tid_model_not_in_group_errors() {
        let group = StoredGroup {
            id: "group_abc".into(),
            entries: vec![entry("gpt-5.2", "api_x")],
        };
        let err =
            plan_resume(Some("group_abc"), &["gemini-2.5-pro".into()], Some(group)).unwrap_err();
        assert!(err.to_string().contains("no remaining member for model"));
    }

    #[test]
    fn plan_resume_group_tid_duplicate_not_enough_matches_errors() {
        let group = StoredGroup {
            id: "group_abc".into(),
            entries: vec![entry("gpt-5.2", "api_x")],
        };
        let err = plan_resume(
            Some("group_abc"),
            &["gpt-5.2".into(), "gpt-5.2".into()],
            Some(group),
        )
        .unwrap_err();
        assert!(err.to_string().contains("no remaining member for model"));
    }

    #[test]
    fn plan_resume_group_tid_not_found_errors() {
        let err = plan_resume(Some("group_missing"), &["gpt-5.2".into()], None).unwrap_err();
        assert!(err.to_string().contains("group thread not found"));
    }
}
