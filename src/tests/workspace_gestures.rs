use bevy::prelude::*;

use super::*;
use crate::commands::{Command, Operation};
use crate::config::Config;
use crate::ecs::ActiveWorkspaceMarker;
use crate::ecs::layout::LayoutStrip;
use crate::events::Event;

fn window_ids(world: &mut World, workspace: u64, row: u32) -> Vec<i32> {
    let entities = world
        .query::<&LayoutStrip>()
        .iter(world)
        .find(|strip| strip.id() == workspace && strip.virtual_index == row)
        .expect("workspace exists")
        .all_windows();
    let mut ids = entities
        .into_iter()
        .map(|entity| world.get::<crate::manager::Window>(entity).unwrap().id())
        .collect::<Vec<_>>();
    ids.sort_unstable();
    ids
}

fn command(op: Operation) -> Event {
    Event::Command {
        command: Command::Window(op),
    }
}

#[test]
fn swapping_workspaces_preserves_groups_focus_and_other_display() {
    use crate::commands::{Direction, MoveFocus};
    let config = Config::try_from("[options]\nreap_empty_workspaces = false").unwrap();
    TestHarness::new()
        .with_config(config)
        .with_windows(4)
        .with_focused_window(0)
        .with_display(
            EXT_DISPLAY_ID,
            IRect::new(0, -EXT_DISPLAY_HEIGHT, EXT_DISPLAY_WIDTH, 0),
            vec![EXT_WORKSPACE_ID],
        )
        .with_workspace_window(100, EXT_WORKSPACE_ID, |_| {})
        .on_iteration(7, |world, _| {
            assert_eq!(active_row(world), 2);
            assert_eq!(window_ids(world, TEST_WORKSPACE_ID, 2), vec![0, 1]);
            assert_eq!(window_ids(world, TEST_WORKSPACE_ID, 1), vec![2]);
            assert_eq!(window_ids(world, TEST_WORKSPACE_ID, 0), vec![3]);
            assert_eq!(window_ids(world, EXT_WORKSPACE_ID, 0), vec![100]);
        })
        .on_iteration(9, |world, _| {
            assert_eq!(active_row(world), 2);
            assert_eq!(window_ids(world, TEST_WORKSPACE_ID, 2), vec![0, 1]);
            assert_eq!(
                world
                    .query::<&LayoutStrip>()
                    .iter(world)
                    .filter(|s| s.id() == TEST_WORKSPACE_ID)
                    .count(),
                3
            );
        })
        .run(vec![
            Event::MenuOpened { window_id: 0 },
            command(Operation::Focus(Direction::First)),
            command(Operation::VirtualMoveNumber(1, MoveFocus::Stay)),
            command(Operation::Focus(Direction::First)),
            command(Operation::VirtualMoveNumber(1, MoveFocus::Stay)),
            command(Operation::VirtualMoveNumber(2, MoveFocus::Follow)),
            command(Operation::VirtualNumber(1)),
            command(Operation::VirtualSwapNumber(2)),
            command(Operation::VirtualSwapNumber(2)),
            command(Operation::VirtualSwapNumber(77)),
        ]);
}

#[test]
fn vertical_display_gesture_uses_geometry_and_only_moves_once_per_touch() {
    let config = Config::try_from(
        r"
[options]
reap_empty_workspaces = false
[swipe]
sensitivity = 1.0
[swipe.gesture]
fingers_count = 3
workspace_fingers_count = 4
display_fingers_count = 4
vertical = false
",
    )
    .unwrap();
    TestHarness::new()
        .with_config(config)
        .with_windows(1)
        .with_focused_window(0)
        // Add a side display before the actual destination to defeat query ordering.
        .with_display(
            3,
            IRect::new(
                TEST_DISPLAY_WIDTH,
                0,
                TEST_DISPLAY_WIDTH * 2,
                TEST_DISPLAY_HEIGHT,
            ),
            vec![30],
        )
        .with_display(
            EXT_DISPLAY_ID,
            IRect::new(0, -EXT_DISPLAY_HEIGHT, EXT_DISPLAY_WIDTH, 0),
            vec![EXT_WORKSPACE_ID],
        )
        .with_display(
            4,
            IRect::new(
                0,
                -EXT_DISPLAY_HEIGHT * 2,
                EXT_DISPLAY_WIDTH,
                -EXT_DISPLAY_HEIGHT,
            ),
            vec![40],
        )
        .on_iteration(0, |world, _| {
            // The destination screen is on workspace 3, independently of the laptop.
            for mut strip in world.query::<&mut LayoutStrip>().iter_mut(world) {
                if strip.id() == EXT_WORKSPACE_ID {
                    strip.virtual_index = 2;
                }
            }
        })
        .on_iteration(2, |world, state| {
            assert_eq!(window_ids(world, EXT_WORKSPACE_ID, 2), vec![0]);
            assert!(
                IRect::new(0, -EXT_DISPLAY_HEIGHT, EXT_DISPLAY_WIDTH, 0)
                    .contains(state.cursor_position())
            );
            // The mock mouse warp records the cursor but doesn't synthesize
            // WindowServer's active-display notification.
            state.activate_display(EXT_DISPLAY_ID);
        })
        .on_iteration(4, |world, state| {
            assert_eq!(window_ids(world, EXT_WORKSPACE_ID, 2), vec![0]);
            assert_eq!(state.active_display(), EXT_DISPLAY_ID);
            assert_eq!(active_row(world), 2);
        })
        .on_iteration(7, |world, state| {
            assert_eq!(window_ids(world, TEST_WORKSPACE_ID, 0), vec![0]);
            assert!(
                IRect::new(0, 0, TEST_DISPLAY_WIDTH, TEST_DISPLAY_HEIGHT)
                    .contains(state.cursor_position())
            );
            state.activate_display(TEST_DISPLAY_ID);
        })
        .on_iteration(10, |world, state| {
            // There is no display below the laptop: do not wrap to another screen.
            assert_eq!(window_ids(world, TEST_WORKSPACE_ID, 0), vec![0]);
            assert_eq!(state.active_display(), TEST_DISPLAY_ID);
        })
        .run(vec![
            Event::MenuOpened { window_id: 0 },
            Event::TouchpadDown,
            Event::VerticalSwipe {
                delta: -0.2,
                fingers: 4,
            },
            Event::VerticalSwipe {
                delta: -0.4,
                fingers: 4,
            },
            swipe(0.4, 4),
            Event::TouchpadUp,
            Event::TouchpadDown,
            Event::VerticalSwipe {
                delta: 0.2,
                fingers: 4,
            },
            Event::TouchpadUp,
            Event::TouchpadDown,
            Event::VerticalSwipe {
                delta: 0.2,
                fingers: 4,
            },
        ]);
}

#[test]
fn horizontal_workspace_gesture_does_not_also_move_window_between_displays() {
    let config = Config::try_from(
        r"
[options]
create_virtual_workspace_automatically = true
[swipe]
sensitivity = 1.0
[swipe.gesture]
workspace_fingers_count = 4
display_fingers_count = 4
",
    )
    .unwrap();
    TestHarness::new()
        .with_config(config)
        .with_windows(1)
        .with_display(
            EXT_DISPLAY_ID,
            IRect::new(0, -EXT_DISPLAY_HEIGHT, EXT_DISPLAY_WIDTH, 0),
            vec![EXT_WORKSPACE_ID],
        )
        .on_iteration(3, |world, state| {
            assert_eq!(state.active_display(), TEST_DISPLAY_ID);
            assert_eq!(active_row(world), 1);
            assert_eq!(window_ids(world, TEST_WORKSPACE_ID, 0), vec![0]);
            assert_eq!(window_ids(world, EXT_WORKSPACE_ID, 0), [] as [i32; 0]);
        })
        .run(vec![
            Event::MenuOpened { window_id: 0 },
            Event::TouchpadDown,
            swipe(0.2, 4),
            Event::VerticalSwipe {
                delta: -0.4,
                fingers: 4,
            },
        ]);
}

fn active_row(world: &mut World) -> u32 {
    world
        .query_filtered::<&LayoutStrip, With<ActiveWorkspaceMarker>>()
        .single(world)
        .unwrap()
        .virtual_index
}

fn swipe(delta: f64, fingers: usize) -> Event {
    Event::Swipe {
        delta,
        fingers,
        distance_cm: delta * 3.0,
    }
}

#[test]
fn short_workspace_swipe_is_independent_of_strip_sensitivity() {
    for sensitivity in [0.1, 0.15, 2.0] {
        let config = Config::try_from(
            format!(
                r"
[options]
create_virtual_workspace_automatically = true
[swipe]
sensitivity = {sensitivity}
[swipe.gesture]
fingers_count = 3
workspace_fingers_count = 4
"
            )
            .as_str(),
        )
        .unwrap();
        TestHarness::new()
            .with_config(config)
            .with_windows(1)
            .on_iteration(2, |world, _| assert_eq!(active_row(world), 0))
            .on_iteration(3, |world, _| assert_eq!(active_row(world), 1))
            .on_iteration(4, |world, _| assert_eq!(active_row(world), 1))
            .run(vec![
                Event::MenuOpened { window_id: 0 },
                Event::TouchpadDown,
                swipe(0.04, 4),
                swipe(0.09, 4),
                swipe(0.4, 4),
            ]);
    }
}

#[test]
fn short_vertical_display_swipe_ignores_low_strip_sensitivity() {
    let config = Config::try_from(
        r"
[swipe]
sensitivity = 0.15
[swipe.gesture]
fingers_count = 3
display_fingers_count = 4
vertical = false
",
    )
    .unwrap();
    TestHarness::new()
        .with_config(config)
        .with_windows(1)
        .with_focused_window(0)
        .with_display(
            EXT_DISPLAY_ID,
            IRect::new(0, -EXT_DISPLAY_HEIGHT, EXT_DISPLAY_WIDTH, 0),
            vec![EXT_WORKSPACE_ID],
        )
        .on_iteration(2, |world, _| {
            assert_eq!(window_ids(world, TEST_WORKSPACE_ID, 0), vec![0]);
        })
        .on_iteration(3, |world, _| {
            assert_eq!(window_ids(world, EXT_WORKSPACE_ID, 0), vec![0]);
        })
        .run(vec![
            Event::MenuOpened { window_id: 0 },
            Event::TouchpadDown,
            Event::VerticalSwipe {
                delta: -0.04,
                fingers: 4,
            },
            Event::VerticalSwipe {
                delta: -0.09,
                fingers: 4,
            },
        ]);
}

#[test]
fn horizontal_workspace_gesture_switches_once_and_resets_on_lift() {
    let config = Config::try_from(
        r"
[options]
create_virtual_workspace_automatically = true
[swipe]
sensitivity = 1.0
[swipe.gesture]
fingers_count = 3
workspace_fingers_count = 4
vertical = false
",
    )
    .unwrap();
    TestHarness::new()
        .with_config(config)
        .with_windows(2)
        .on_iteration(2, |world, _| assert_eq!(active_row(world), 1))
        .on_iteration(3, |world, _| assert_eq!(active_row(world), 1))
        .on_iteration(6, |world, _| assert_eq!(active_row(world), 0))
        .on_iteration(8, |world, _| assert_eq!(active_row(world), 0))
        .run(vec![
            Event::MenuOpened { window_id: 0 },
            Event::TouchpadDown,
            swipe(0.2, 4),
            swipe(0.3, 4),
            Event::TouchpadUp,
            Event::TouchpadDown,
            swipe(-0.2, 4),
            Event::TouchpadUp,
            swipe(0.2, 3),
        ]);
}

#[test]
fn workspace_gesture_takes_precedence_over_paging_with_same_finger_count() {
    let config = Config::try_from(
        r"
[options]
create_virtual_workspace_automatically = true
[swipe]
sensitivity = 1.0
[swipe.gesture]
fingers_count = 4
workspace_fingers_count = 4
paging = true
",
    )
    .unwrap();
    TestHarness::new()
        .with_config(config)
        .with_windows(3)
        .on_iteration(1, |world, _| assert_eq!(active_row(world), 1))
        .run(vec![Event::MenuOpened { window_id: 0 }, swipe(1.0, 4)]);
}

#[test]
fn horizontal_workspace_gesture_does_not_switch_other_display() {
    let config = Config::try_from(
        r"
[swipe]
sensitivity = 1.0
[swipe.gesture]
fingers_count = 3
workspace_fingers_count = 4
",
    )
    .unwrap();
    TestHarness::new()
        .with_config(config)
        .with_display(
            EXT_DISPLAY_ID,
            IRect::new(0, -EXT_DISPLAY_HEIGHT, EXT_DISPLAY_WIDTH, 0),
            vec![EXT_WORKSPACE_ID],
        )
        .with_window(0, |_| {})
        .with_workspace_window(100, EXT_WORKSPACE_ID, |_| {})
        .on_iteration(3, |world, _| {
            assert_eq!(active_row(world), 1);
            let mut strips = world.query::<&LayoutStrip>();
            let top = strips
                .iter(world)
                .filter(|strip| strip.id() == EXT_WORKSPACE_ID)
                .collect::<Vec<_>>();
            assert_eq!(top.len(), 1);
            assert_eq!(top[0].virtual_index, 0);
            assert_eq!(top[0].all_windows().len(), 1);
        })
        .run(vec![
            Event::MenuOpened { window_id: 0 },
            Event::Command {
                command: Command::Window(Operation::VirtualNumber(1)),
            },
            Event::Command {
                command: Command::Window(Operation::VirtualNumber(0)),
            },
            swipe(0.2, 4),
        ]);
}
