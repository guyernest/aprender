//! The binary contract (design §5.1, §6.2): argv dispatch, exit codes, the precheck writer.

use super::cases;
use crate::precheck::Env;
use crate::store::MemStore;
use crate::{dispatch, run_precheck, Mode, EXIT_USAGE};

fn argv(a: &[&str]) -> Vec<String> {
    a.iter().map(|s| (*s).to_string()).collect()
}

#[test]
fn dispatch_table() {
    assert!(matches!(dispatch(&argv(&["bootstrap"])), Mode::Serve));
    assert!(matches!(
        dispatch(&argv(&["bootstrap", "--serve-local"])),
        Mode::ServeLocal
    ));
    assert!(matches!(
        dispatch(&argv(&["bootstrap", "precheck"])),
        Mode::Precheck
    ));
    assert!(matches!(
        dispatch(&argv(&["bootstrap", "--help"])),
        Mode::Usage
    ));
    assert!(matches!(
        dispatch(&argv(&["bootstrap", "precheck", "--case", "P1"])),
        Mode::Usage
    ));
    assert!(matches!(dispatch(&argv(&[])), Mode::Usage));
    assert_eq!(EXIT_USAGE, 64);
}

#[tokio::test]
async fn run_precheck_writes_the_grammar_and_returns_the_exit() {
    let mut out = Vec::new();
    let code = run_precheck(
        &cases::staging_config().replace("[model]\n", "[model]\nmemory_mb = 10240\n"),
        cases::SERVER_ID,
        &Env::default(),
        &MemStore::default(),
        &mut out,
    )
    .await;
    assert_eq!(code, 3);
    let s = String::from_utf8(out).expect("utf8");
    assert!(
        s.starts_with("ERROR: REFUSED config-invalid: ")
            && s.ends_with('\n')
            && s.lines().count() == 1,
        "{s}"
    );
    let case = cases::all()
        .into_iter()
        .find(|c| c.name == "P1")
        .expect("P1");
    let mut out = Vec::new();
    assert_eq!(
        run_precheck(
            &case.config,
            case.expected_id,
            &case.env,
            &case.store,
            &mut out
        )
        .await,
        0
    );
    assert!(String::from_utf8(out)
        .expect("utf8")
        .starts_with("ELIGIBLE {\"config_sha256\":"));
}
