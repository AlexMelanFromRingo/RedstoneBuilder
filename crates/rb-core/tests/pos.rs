#![allow(clippy::unwrap_used, clippy::expect_used)]

use rb_core::{Bbox3, Direction, Pos3};

#[test]
fn pos_translate_steps_in_each_direction() {
    let p = Pos3::ORIGIN;
    assert_eq!(p.step(Direction::East), Pos3::new(1, 0, 0));
    assert_eq!(p.step(Direction::West), Pos3::new(-1, 0, 0));
    assert_eq!(p.step(Direction::Up), Pos3::new(0, 1, 0));
    assert_eq!(p.step(Direction::Down), Pos3::new(0, -1, 0));
    assert_eq!(p.step(Direction::North), Pos3::new(0, 0, -1));
    assert_eq!(p.step(Direction::South), Pos3::new(0, 0, 1));
}

#[test]
fn neighbors4_excludes_self_and_vertical() {
    let p = Pos3::new(5, 5, 5);
    let n = p.neighbors4();
    assert_eq!(n.len(), 4);
    assert!(!n.contains(&p));
    assert!(n.iter().all(|q| q.y == p.y));
}

#[test]
fn bbox_contains_inclusive_corners() {
    let b = Bbox3::from_corners(Pos3::new(0, 0, 0), Pos3::new(10, 5, 8));
    assert!(b.contains(Pos3::new(0, 0, 0)));
    assert!(b.contains(Pos3::new(10, 5, 8)));
    assert!(b.contains(Pos3::new(5, 3, 4)));
    assert!(!b.contains(Pos3::new(11, 0, 0)));
    assert!(!b.contains(Pos3::new(0, -1, 0)));
}

#[test]
fn bbox_dimensions_and_volume() {
    let b = Bbox3::from_corners(Pos3::new(0, 0, 0), Pos3::new(3, 1, 2));
    assert_eq!(b.width(), 4);
    assert_eq!(b.height(), 2);
    assert_eq!(b.depth(), 3);
    assert_eq!(b.volume(), 24);
}

#[test]
fn bbox_from_corners_handles_unordered_input() {
    let a = Bbox3::from_corners(Pos3::new(10, 5, 8), Pos3::new(0, 0, 0));
    let b = Bbox3::from_corners(Pos3::new(0, 0, 0), Pos3::new(10, 5, 8));
    assert_eq!(a, b);
}

#[test]
fn bbox_union_expands_to_cover_both() {
    let a = Bbox3::from_corners(Pos3::new(0, 0, 0), Pos3::new(5, 5, 5));
    let b = Bbox3::from_corners(Pos3::new(3, 3, 3), Pos3::new(10, 10, 10));
    let u = a.union(&b);
    assert_eq!(u.min, Pos3::new(0, 0, 0));
    assert_eq!(u.max, Pos3::new(10, 10, 10));
}

#[test]
fn direction_offset_matches_minecraft_convention() {
    // Minecraft convention: +X east, +Y up, +Z south.
    assert_eq!(Direction::East.offset(), (1, 0, 0));
    assert_eq!(Direction::Up.offset(), (0, 1, 0));
    assert_eq!(Direction::South.offset(), (0, 0, 1));
}

#[test]
fn direction_str_matches_nbt_property_values() {
    assert_eq!(Direction::North.as_str(), "north");
    assert_eq!(Direction::East.as_str(), "east");
    assert_eq!(Direction::Up.as_str(), "up");
}
