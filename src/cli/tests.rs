use super::TaskArg;
use super::run::build_args;
use crate::schema::TaskMode;

fn bare_cli(files: Vec<String>, diff_files: Vec<String>) -> super::Cli {
    super::Cli {
        json: false,
        cmd: None,
        model: vec![],
        files,
        thread_id: None,
        task: TaskArg::General,
        web: false,
        prompt_file: None,
        diff_files,
        diff_base: None,
        diff_repo: None,
        runs: vec![],
    }
}

#[test]
fn diff_args_only_when_files_given() {
    let a = build_args(&bare_cli(vec![], vec![]), "p".into());
    assert!(a.git_diff.is_none());
    let b = build_args(&bare_cli(vec![], vec!["f.rs".into()]), "p".into());
    assert_eq!(b.git_diff.unwrap().files, vec!["f.rs".to_string()]);
}

#[test]
fn task_arg_maps() {
    assert!(matches!(TaskMode::from(TaskArg::Review), TaskMode::Review));
    assert!(matches!(TaskMode::from(TaskArg::Plan), TaskMode::Plan));
    assert!(matches!(TaskMode::from(TaskArg::Debug), TaskMode::Debug));
    assert!(matches!(TaskMode::from(TaskArg::Create), TaskMode::Create));
    assert!(matches!(
        TaskMode::from(TaskArg::General),
        TaskMode::General
    ));
}
