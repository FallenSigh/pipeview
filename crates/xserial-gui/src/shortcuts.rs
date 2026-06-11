use egui::{Context, Key, KeyboardShortcut, Modifiers};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    NextTab,
    PrevTab,

    ViewText,
    ViewHex,
    ViewPlot,

    NewSession,
    EditSession,
    DeleteSession,
    ToggleConnect,
    Clear,

    UiSettings,
    CloseOverlay,
}

pub fn default_bindings() -> Vec<(Action, KeyboardShortcut)> {
    use Action::*;

    vec![
        (NextTab, KeyboardShortcut::new(Modifiers::CTRL, Key::Tab)),
        (
            PrevTab,
            KeyboardShortcut::new(Modifiers::CTRL | Modifiers::SHIFT, Key::Tab),
        ),
        (ViewText, KeyboardShortcut::new(Modifiers::CTRL, Key::T)),
        (ViewHex, KeyboardShortcut::new(Modifiers::CTRL, Key::H)),
        (ViewPlot, KeyboardShortcut::new(Modifiers::CTRL, Key::P)),
        (NewSession, KeyboardShortcut::new(Modifiers::CTRL, Key::N)),
        (EditSession, KeyboardShortcut::new(Modifiers::CTRL, Key::E)),
        (
            DeleteSession,
            KeyboardShortcut::new(Modifiers::CTRL, Key::W),
        ),
        (
            ToggleConnect,
            KeyboardShortcut::new(Modifiers::CTRL, Key::F5),
        ),
        (Clear, KeyboardShortcut::new(Modifiers::CTRL, Key::L)),
        (
            UiSettings,
            KeyboardShortcut::new(Modifiers::CTRL, Key::Comma),
        ),
        (
            CloseOverlay,
            KeyboardShortcut::new(Modifiers::NONE, Key::Escape),
        ),
    ]
}

pub fn process(
    ctx: &Context,
    suppress_char_shortcuts: bool,
    bindings: &[(Action, KeyboardShortcut)],
) -> Vec<Action> {
    let mut actions = Vec::new();

    ctx.input_mut(|input| {
        for (action, shortcut) in bindings {
            if !input.consume_shortcut(shortcut) {
                continue;
            }

            if suppress_char_shortcuts && is_char_based(*action) {
                continue;
            }

            actions.push(*action);
        }
    });

    actions
}

fn is_char_based(action: Action) -> bool {
    use Action::*;
    !matches!(action, CloseOverlay)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bindings_contain_no_duplicate_shortcuts() {
        let bindings = default_bindings();
        for (i, (_, a)) in bindings.iter().enumerate() {
            for (j, (_, b)) in bindings.iter().enumerate() {
                if i != j {
                    assert_ne!(a, b, "Duplicate shortcut at indices {i} and {j}");
                }
            }
        }
    }

    #[test]
    fn bindings_non_empty() {
        assert!(!default_bindings().is_empty());
    }

    #[test]
    fn close_overlay_not_char_based() {
        assert!(!is_char_based(Action::CloseOverlay));
    }

    #[test]
    fn all_other_actions_are_char_based() {
        for action in [
            Action::NextTab,
            Action::PrevTab,
            Action::ViewText,
            Action::ViewHex,
            Action::ViewPlot,
            Action::NewSession,
            Action::EditSession,
            Action::DeleteSession,
            Action::ToggleConnect,
            Action::Clear,
            Action::UiSettings,
        ] {
            assert!(is_char_based(action), "{action:?} should be char-based");
        }
    }
}
