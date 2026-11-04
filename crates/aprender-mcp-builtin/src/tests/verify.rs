//! `verify`'s door and its pure parts (design §4.4, §4.6).

use pmcp::RequestHandlerExtra;

use crate::tools::{max_abs_diff, permission_denied, read_eval_rows, select_rows, verify_allowed};

fn extra_with(subject: Option<&str>, groups: &[&str]) -> RequestHandlerExtra {
    // pmcp 2.19.3: RequestHandlerExtra::new(request_id, CancellationToken).with_auth_context(..);
    // AuthContext::new(subject) with the public `claims` map; groups() reads the `groups` claim.
    let ctx = subject.map(|s| {
        let mut c = pmcp::server::auth::AuthContext::new(s);
        c.claims.insert("groups".into(), serde_json::json!(groups));
        c
    });
    RequestHandlerExtra::new("req-1".into(), tokio_util::sync::CancellationToken::new())
        .with_auth_context(ctx)
}

#[test]
fn door_denies_without_identity() {
    assert!(!verify_allowed(
        &extra_with(None, &[]),
        "admin",
        &["m2m".into()]
    ));
}

#[test]
fn door_admits_the_role_group() {
    assert!(verify_allowed(
        &extra_with(Some("sub"), &["analysts", "admin"]),
        "admin",
        &[]
    ));
    assert!(!verify_allowed(
        &extra_with(Some("sub"), &["analysts"]),
        "admin",
        &[]
    ));
}

#[test]
fn door_admits_an_allow_listed_principal_without_groups() {
    assert!(verify_allowed(
        &extra_with(Some("3k9f2m2mclientid"), &[]),
        "admin",
        &["3k9f2m2mclientid".into()]
    ));
    assert!(
        !verify_allowed(
            &extra_with(Some("3k9f2m2mclientid-x"), &[]),
            "admin",
            &["3k9f2m2mclientid".into()]
        ),
        "exact equality only"
    );
}

#[test]
fn denial_text_names_the_configured_role() {
    assert!(permission_denied("decider-admin")
        .to_string()
        .contains("Permission denied: verify requires 'decider-admin' role"));
}

#[test]
fn select_rows_is_deterministic_sorted_and_bounded() {
    let a = select_rows(2129, 8, 0);
    let b = select_rows(2129, 8, 0);
    assert_eq!(a, b);
    assert_eq!(a.len(), 8);
    assert!(a.windows(2).all(|w| w[0] < w[1]));
    assert!(a.iter().all(|&i| i < 2129));
    assert_ne!(select_rows(2129, 8, 1), a);
    assert_eq!(select_rows(3, 3, 7), vec![0, 1, 2]);
}

#[test]
fn eval_rows_skip_blank_lines_and_strip_cr() {
    let rows = read_eval_rows(
        b"{\"text\":\"a\",\"label\":\"x\"}\r\n\n{\"text\":\"b\",\"label\":\"y\"}\n\n",
    )
    .expect("rows");
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].text, "a");
    let e = read_eval_rows(b"{\"text\":\"a\",\"label\":\"x\"}\nnot json\n").expect_err("line 2");
    assert!(e.contains("line 2"), "{e}");
}

#[test]
fn max_abs_diff_is_nan_visible() {
    assert!((max_abs_diff(&[0.5, 0.5], &[0.5, 0.4]) - 0.1).abs() < 1e-6);
    assert!(max_abs_diff(&[f32::NAN, 0.5], &[0.5, 0.5]).is_nan());
    assert!(
        max_abs_diff(&[0.5], &[0.5, 0.5]).is_nan(),
        "length mismatch is NaN"
    );
}
