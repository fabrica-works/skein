use crate::outline::{Fault, Rule, check};
use crate::tests::fixtures::{LIMITS, finish, page};
use crate::{Binding, Builder, Modifier, Owner, Role, Shortcut};
use skein_lib::Token;

const FAULTS: &[(&str, Role, &[Role], Option<Fault>)] = &[
    ("root", Role::Region, &[Role::Main], Some(Fault { rule: Rule::PageRoot, node: 0 })),
    ("outside", Role::Page, &[Role::Main, Role::Text], Some(Fault { rule: Rule::InsideLandmark, node: 3 })),
    ("unnamed region", Role::Page, &[Role::Main, Role::Region], Some(Fault { rule: Rule::InsideLandmark, node: 3 })),
    ("empty region name", Role::Page, &[Role::Main, Role::Region], Some(Fault { rule: Rule::InsideLandmark, node: 3 })),
    ("second main", Role::Page, &[Role::Main, Role::Main], Some(Fault { rule: Rule::OneMain, node: 3 })),
    ("missing main", Role::Page, &[], Some(Fault { rule: Rule::OneMain, node: 0 })),
    ("second banner", Role::Page, &[Role::Main, Role::Banner], Some(Fault { rule: Rule::OneBanner, node: 3 })),
    (
        "second content info",
        Role::Page,
        &[Role::Main, Role::ContentInfo, Role::ContentInfo],
        Some(Fault { rule: Rule::OneContentInfo, node: 4 }),
    ),
    ("shortcut", Role::Page, &[Role::Main], Some(Fault { rule: Rule::ShortcutOnce, node: 4 })),
    ("escape main", Role::Page, &[Role::Main], Some(Fault { rule: Rule::EscapeInDialog, node: 3 })),
    ("escape dialog", Role::Page, &[Role::Main, Role::Dialog], None),
];

#[test]
fn each_rule_names_its_first_fault() {
    for (label, root, roles, fault) in FAULTS {
        let mut b = page();
        for role in *roles {
            b.open(*role);
            if label == &"empty region name" && *role == Role::Region {
                b.name(b"");
            }
            if (label == &"escape main" && *role == Role::Main) || (label == &"escape dialog" && *role == Role::Dialog)
            {
                b.open(Role::Button);
                b.bind(Binding { owner: Owner { number: 1 }, token: Token::new(1) });
                b.shortcut(Shortcut::ESCAPE);
                b.close();
            }
            if label == &"shortcut" {
                for _ in 0_u8..2 {
                    b.open(Role::Button);
                    b.bind(Binding { owner: Owner { number: 1 }, token: Token::new(1) });
                    b.shortcut(Shortcut { key: b'a', modifier: Modifier::Primary });
                    b.close();
                }
            }
            b.close();
        }
        let mut tree = finish(b);
        tree.nodes.get_mut(0).expect("page root").role = *root;
        assert_eq!(check(&tree), *fault, "{label}");
    }
    let empty = Builder::new(&LIMITS).finish();
    assert_eq!(check(&empty), Some(Fault { rule: Rule::PageRoot, node: 0 }), "empty tree");
}

#[test]
fn a_page_keeping_its_outline_has_no_fault() {
    let mut b = page();
    for role in [Role::Navigation, Role::Main, Role::Complementary, Role::ContentInfo] {
        b.open(role);
        b.close();
    }
    assert_eq!(check(&finish(b)), None);
}

#[test]
fn a_page_with_form_search_and_a_dialog_is_whole() {
    let mut b = page();
    for role in [Role::Main, Role::Form, Role::Search, Role::Dialog, Role::Region] {
        b.open(role);
        b.name(b"Named landmark");
        b.close();
    }
    assert_eq!(check(&finish(b)), None);
}
