const LAYOUT: &str = include_str!("../templates/layout.html");
const NAV: &str = include_str!("../templates/nav.html");
const PIPELINE_BOARD: &str = include_str!("../templates/pipeline_board.html");
const AUTONOMY: &str = include_str!("../templates/autonomy.html");
const FLASH: &str = include_str!("../templates/components/flash.html");

fn assert_no_nested_forms(name: &str, template: &str) {
    let mut open_forms = 0;
    for line in template.lines() {
        if line.contains("<form ") {
            assert_eq!(open_forms, 0, "{name} contains nested forms");
            open_forms += 1;
        }
        if line.contains("</form>") {
            assert_eq!(open_forms, 1, "{name} closes a form that is not open");
            open_forms -= 1;
        }
    }
    assert_eq!(open_forms, 0, "{name} leaves a form open");
}

#[test]
fn shell_accessibility_contract() {
    assert!(LAYOUT.contains("<html lang=\"en\">"));
    assert!(LAYOUT.contains("<a class=\"skip-link\" href=\"#main-content\">"));
    assert!(LAYOUT.contains("<aside class=\"sidebar\" aria-label=\"Application sidebar\">"));
    assert!(NAV.contains("<nav class=\"sidebar-nav\" aria-label=\"Main navigation\">"));
    assert!(LAYOUT.contains("<main id=\"main-content\""));
    assert!(LAYOUT.contains("<footer class=\"app-footer\" role=\"contentinfo\">"));

    assert!(PIPELINE_BOARD.contains("<details class=\"new-deal-disclosure\">"));
    assert!(PIPELINE_BOARD.contains("<summary class=\"btn btn-primary\">+ New Deal</summary>"));
    assert!(!PIPELINE_BOARD.contains("onclick="));

    assert!(AUTONOMY.contains("<details class=\"kind-drawer-disclosure\">"));
    assert!(AUTONOMY.contains("<summary class=\"btn btn-outline\">Kind Overrides</summary>"));
    assert!(!AUTONOMY.contains("onclick="));

    assert!(FLASH.contains("<button type=\"button\""));

    for (name, template) in [
        ("pipeline board", PIPELINE_BOARD),
        ("autonomy", AUTONOMY),
        (
            "action button",
            include_str!("../templates/action_button.html"),
        ),
        (
            "approval row",
            include_str!("../templates/approval_row.html"),
        ),
        ("approvals", include_str!("../templates/approvals.html")),
        ("bridges", include_str!("../templates/bridges.html")),
        ("record view", include_str!("../templates/record_view.html")),
    ] {
        assert_no_nested_forms(name, template);
        if template.contains("hx-post=") {
            assert!(
                template.contains("<form action="),
                "{name} lost native form action"
            );
            assert!(
                template.contains("method=\"post\""),
                "{name} lost native POST method"
            );
        }
    }

    println!("shell accessibility contract: ok");
}
