//! Tests for `fullscreen_workspaces` and `window_fullscreen`: a window going
//! full screen fills a virtual workspace of its own instead of a native macOS
//! full-screen Space, and going full screen again puts it back.

use bevy::prelude::*;

use crate::commands::{Command, Operation};
use crate::config::{Config, MainOptions};
use crate::ecs::workspace::PaneruFullscreen;
use crate::ecs::{ActiveWorkspaceMarker, FullWidthMarker, layout::LayoutStrip};
use crate::events::Event;

use super::*;

const FULLSCREEN_WORKSPACE_ID: WorkspaceId = TEST_WORKSPACE_ID + 100;

fn fullscreen_workspaces_config() -> Config {
    let options = MainOptions {
        fullscreen_workspaces: Some(true),
        ..MainOptions::default()
    };
    (options, vec![]).into()
}

/// The virtual index of the strip holding `entity`, and whether it is active.
fn row_of(world: &mut World, entity: Entity) -> (u32, bool) {
    let mut strips = world.query::<(&LayoutStrip, Has<ActiveWorkspaceMarker>)>();
    strips
        .iter(world)
        .find(|(strip, _)| strip.contains(entity))
        .map(|(strip, active)| (strip.virtual_index, active))
        .expect("window is in a strip")
}

fn width_of(world: &mut World, entity: Entity) -> i32 {
    world
        .get::<crate::manager::Window>(entity)
        .expect("window")
        .frame()
        .width()
}

fn is_paneru_fullscreen(world: &mut World, entity: Entity) -> bool {
    let entity = world.entity(entity);
    entity.contains::<PaneruFullscreen>() && entity.contains::<FullWidthMarker>()
}

#[test]
fn native_fullscreen_becomes_a_workspace_of_its_own_and_returns() {
    TestHarness::new()
        .with_config(fullscreen_workspaces_config())
        .with_windows(2)
        .with_focused_window(0)
        .on_iteration(0, |_world, state| {
            // The user makes window 0 native full screen: macOS moves it to a
            // Space of its own and switches there.
            state.update_window(0, |window| {
                window.workspace_id = FULLSCREEN_WORKSPACE_ID;
                window.is_full_screen = true;
            });
            state.activate_workspace(TEST_DISPLAY_ID, FULLSCREEN_WORKSPACE_ID, true);
        })
        .on_iteration(1, |world, state| {
            let window = find_window_entity(0, world);
            assert_eq!(
                state.exit_full_screen_requests(0),
                1,
                "Paneru takes the window out of native full screen"
            );
            assert!(
                world
                    .query::<&LayoutStrip>()
                    .iter(world)
                    .all(|strip| strip.id() != FULLSCREEN_WORKSPACE_ID),
                "no strip is made for the native full-screen Space"
            );
            assert!(
                world
                    .query::<&LayoutStrip>()
                    .iter(world)
                    .any(|strip| strip.contains(window)),
                "the window stays in the layout"
            );

            // macOS animates back to the window's Space.
            state.update_window(0, |window| window.workspace_id = TEST_WORKSPACE_ID);
            state.activate_workspace(TEST_DISPLAY_ID, TEST_WORKSPACE_ID, false);
        })
        .on_iteration(3, |world, _state| {
            let window = find_window_entity(0, world);
            let sibling = find_window_entity(1, world);
            assert_eq!(row_of(world, window), (1, true), "own workspace, shown");
            assert_eq!(row_of(world, sibling).0, 0);
            assert!(is_paneru_fullscreen(world, window));
            assert_eq!(
                width_of(world, window),
                TEST_DISPLAY_WIDTH,
                "fills the screen"
            );
        })
        .on_iteration(5, |world, _state| {
            let window = find_window_entity(0, world);
            assert_eq!(row_of(world, window), (0, true), "back in its workspace");
            assert_eq!(
                width_of(world, window),
                TEST_WINDOW_WIDTH,
                "at its old width"
            );
            assert!(!world.entity(window).contains::<PaneruFullscreen>());
            assert!(!world.entity(window).contains::<FullWidthMarker>());
        })
        .run(vec![
            Event::Command {
                command: Command::PrintState,
            },
            Event::SpaceChanged,
            Event::SpaceChanged,
            Event::Command {
                command: Command::PrintState,
            },
            Event::Command {
                command: Command::Window(Operation::Fullscreen),
            },
            Event::Command {
                command: Command::PrintState,
            },
        ]);
}

#[test]
fn fullscreen_command_fills_the_workspace_of_a_lone_window_in_place() {
    TestHarness::new()
        .with_windows(1)
        .with_focused_window(0)
        .on_iteration(2, |world, state| {
            let window = find_window_entity(0, world);
            assert_eq!(row_of(world, window), (0, true));
            assert!(is_paneru_fullscreen(world, window));
            assert_eq!(
                state.exit_full_screen_requests(0),
                0,
                "a window not in native full screen is not asked to leave it"
            );
        })
        .on_iteration(4, |world, _state| {
            let window = find_window_entity(0, world);
            assert_eq!(row_of(world, window), (0, true));
            assert!(!world.entity(window).contains::<PaneruFullscreen>());
            assert!(!world.entity(window).contains::<FullWidthMarker>());
        })
        .run(vec![
            Event::Command {
                command: Command::PrintState,
            },
            Event::Command {
                command: Command::Window(Operation::Fullscreen),
            },
            Event::Command {
                command: Command::PrintState,
            },
            Event::Command {
                command: Command::Window(Operation::Fullscreen),
            },
            Event::Command {
                command: Command::PrintState,
            },
        ]);
}

#[test]
fn native_fullscreen_keeps_its_space_without_the_option() {
    TestHarness::new()
        .with_windows(2)
        .on_iteration(0, |_world, state| {
            state.update_window(0, |window| {
                window.workspace_id = FULLSCREEN_WORKSPACE_ID;
                window.is_full_screen = true;
            });
            state.activate_workspace(TEST_DISPLAY_ID, FULLSCREEN_WORKSPACE_ID, true);
        })
        .on_iteration(1, |world, state| {
            assert_eq!(state.exit_full_screen_requests(0), 0);
            assert!(
                world
                    .query::<&LayoutStrip>()
                    .iter(world)
                    .any(|strip| strip.id() == FULLSCREEN_WORKSPACE_ID)
            );
        })
        .run(vec![
            Event::Command {
                command: Command::PrintState,
            },
            Event::SpaceChanged,
        ]);
}

#[test]
fn fullscreen_reuses_the_next_empty_workspace() {
    TestHarness::new()
        .with_windows(2)
        .with_focused_window(0)
        .on_iteration(2, |world, _state| {
            let rows = world.query::<&LayoutStrip>().iter(world).count();
            assert_eq!(rows, 2, "workspace 2 stays, empty, after leaving it");
        })
        .on_iteration(4, |world, _state| {
            let focused = world
                .query_filtered::<Entity, With<crate::ecs::FocusedMarker>>()
                .single(world)
                .expect("a focused window");
            assert_eq!(row_of(world, focused), (1, true), "the empty workspace");
            let rows = world.query::<&LayoutStrip>().iter(world).count();
            assert_eq!(rows, 2, "no workspace is added");
        })
        .run(vec![
            Event::Command {
                command: Command::PrintState,
            },
            Event::Command {
                command: Command::Window(Operation::VirtualNumber(1)),
            },
            Event::Command {
                command: Command::Window(Operation::VirtualNumber(0)),
            },
            Event::Command {
                command: Command::Window(Operation::Fullscreen),
            },
            Event::Command {
                command: Command::PrintState,
            },
        ]);
}
