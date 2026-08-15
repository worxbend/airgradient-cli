//! The leader-key popup: what arms it, what cancels it, and the bindings it
//! advertises.

use super::*;

#[test]
fn leader_toggles_and_cancels() {
    let mut app = app_with_refresh(Duration::from_secs(30));
    assert!(!app.leader_pending);

    app.toggle_leader();
    assert!(app.leader_pending);

    // A second <Space> backs out, as which-key does.
    app.toggle_leader();
    assert!(!app.leader_pending);

    app.toggle_leader();
    app.cancel_leader();
    assert!(!app.leader_pending);
}

#[test]
fn leader_resolves_bound_keys_and_dismisses_unbound_ones() {
    let mut app = app_with_refresh(Duration::from_secs(30));

    app.toggle_leader();
    assert_eq!(app.resolve_leader('t'), LeaderAction::OpenThemes);
    assert!(!app.leader_pending, "resolving disarms the leader");

    app.toggle_leader();
    assert_eq!(app.resolve_leader('q'), LeaderAction::Quit);

    app.toggle_leader();
    assert_eq!(app.resolve_leader('z'), LeaderAction::Dismiss);
    assert!(!app.leader_pending);
}

#[test]
fn every_advertised_leader_binding_resolves_to_an_action() {
    // The which-key popup is a promise: anything it lists must actually work.
    let mut app = app_with_refresh(Duration::from_secs(30));

    for binding in LEADER_BINDINGS {
        assert_ne!(
            app.resolve_leader(binding.key),
            LeaderAction::Dismiss,
            "leader binding {:?} is advertised but unbound",
            binding.key
        );
    }
}
