# EW0 · Editor workspace shell

**Status:** Ready for implementation · **Depends on:** none · **Blocks:** EW1, EW2

## User outcome

The user can open the Project and Log panels from both the scene launcher menu and the running editor, adjust the bottom panel size, and keep using the scene hierarchy, viewport, and inspector. Selecting a resource for inspection preserves the current scene entity selection.

## Scope

- Add a bottom workspace with `Project` and `Log` tabs, visibility controls, and a resizable height. First versions of those panels may show a small empty/loading placeholder until EW1/EW2 land.
- Keep it available in both `LauncherState::Menu` and `LauncherState::Running`.
- Preserve panel selection/visibility/size for the session; persist preferences if there is an existing suitable settings location.
- Fullscreen mode hides and restores panels without losing their state.
- Model resource selection separately from `Selected` scene entities.
- Route keyboard shortcuts only when egui does not own keyboard focus.

## TDD acceptance

Write and run these tests before production changes:

1. A workspace-state test proves `Project` and `Log` are selectable and state survives a frame transition.
2. An egui input test proves a Menu error can be shown in Log and Project/Log tabs are accessible from Menu and Running.
3. A selection-state test proves selecting/closing a resource detail preserves the selected Scene entity and Inspector target.
4. A keyboard input test proves Delete in a focused search field does not enqueue `pending_despawn_entity`, while the existing scene shortcut still works when focus is outside UI.
5. A fullscreen transition test proves hidden panels restore to the prior selected tab and dimensions.

For each, record RED output showing a missing behavior, implement the smallest state/UI change, then record GREEN output. Keep rendering orchestration out of test helpers where a pure state transition is sufficient.

## Constraints

- Do not add a general-purpose docking dependency.
- Preserve full-screen screenshot mode (`--no-gui-overlay`) and scene viewport sizing behavior.
- Do not let resource selection mutate `hecs::Selected`.
- New Rust modules remain under 500 lines. Run `cargo fmt`, focused launcher tests, then the repository CI and a Release launcher scene startup check.

## Deliverables

- Workspace panel state and UI integration.
- Passing tests for tab state, Menu access, selection preservation, shortcut focus, and fullscreen restore.
- Manual Release evidence that Menu opens Project/Log and a Running scene still has working viewport/Scene/Inspector.
