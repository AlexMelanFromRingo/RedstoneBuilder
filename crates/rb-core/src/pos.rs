//! 3D positions, bounding boxes, and cardinal/vertical directions —
//! used by placement, routing, and the NBT writer.

use serde::{Deserialize, Serialize};

/// A 3D Minecraft world position, in block units.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Pos3 {
    /// East-west axis (+X = east).
    pub x: i32,
    /// Vertical axis (+Y = up).
    pub y: i32,
    /// North-south axis (+Z = south).
    pub z: i32,
}

impl Pos3 {
    /// Construct a new position.
    pub const fn new(x: i32, y: i32, z: i32) -> Self {
        Self { x, y, z }
    }

    /// Position at the origin (0, 0, 0).
    pub const ORIGIN: Self = Self::new(0, 0, 0);

    /// Translate by a `(dx, dy, dz)` offset (saturating to avoid overflow).
    pub fn translate(self, dx: i32, dy: i32, dz: i32) -> Self {
        Self::new(
            self.x.saturating_add(dx),
            self.y.saturating_add(dy),
            self.z.saturating_add(dz),
        )
    }

    /// Step one block in the given direction.
    pub fn step(self, dir: Direction) -> Self {
        let (dx, dy, dz) = dir.offset();
        self.translate(dx, dy, dz)
    }

    /// The four lateral neighbors at the same Y level.
    pub fn neighbors4(self) -> [Self; 4] {
        [
            self.step(Direction::North),
            self.step(Direction::South),
            self.step(Direction::East),
            self.step(Direction::West),
        ]
    }
}

/// Cardinal + vertical directions used by routing and block-state
/// `facing` properties.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Direction {
    /// -Z
    North,
    /// +Z
    South,
    /// +X
    East,
    /// -X
    West,
    /// +Y
    Up,
    /// -Y
    Down,
}

impl Direction {
    /// All six directions, in a fixed order suitable for deterministic
    /// iteration.
    pub const ALL: [Direction; 6] = [
        Direction::North,
        Direction::South,
        Direction::East,
        Direction::West,
        Direction::Up,
        Direction::Down,
    ];

    /// Unit-vector offset for this direction.
    pub const fn offset(self) -> (i32, i32, i32) {
        match self {
            Direction::North => (0, 0, -1),
            Direction::South => (0, 0, 1),
            Direction::East => (1, 0, 0),
            Direction::West => (-1, 0, 0),
            Direction::Up => (0, 1, 0),
            Direction::Down => (0, -1, 0),
        }
    }

    /// String name suitable for use as an NBT block-state property value.
    pub const fn as_str(self) -> &'static str {
        match self {
            Direction::North => "north",
            Direction::South => "south",
            Direction::East => "east",
            Direction::West => "west",
            Direction::Up => "up",
            Direction::Down => "down",
        }
    }
}

/// Inclusive 3D axis-aligned bounding box.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Bbox3 {
    /// Inclusive minimum corner.
    pub min: Pos3,
    /// Inclusive maximum corner.
    pub max: Pos3,
}

impl Bbox3 {
    /// Construct a bbox containing a single point.
    pub const fn point(p: Pos3) -> Self {
        Self { min: p, max: p }
    }

    /// Construct from two corners; the order does not matter.
    pub fn from_corners(a: Pos3, b: Pos3) -> Self {
        Self {
            min: Pos3::new(a.x.min(b.x), a.y.min(b.y), a.z.min(b.z)),
            max: Pos3::new(a.x.max(b.x), a.y.max(b.y), a.z.max(b.z)),
        }
    }

    /// True if `p` is inside (inclusive on all faces).
    pub fn contains(&self, p: Pos3) -> bool {
        p.x >= self.min.x
            && p.x <= self.max.x
            && p.y >= self.min.y
            && p.y <= self.max.y
            && p.z >= self.min.z
            && p.z <= self.max.z
    }

    /// Width along X (always positive).
    pub fn width(&self) -> u32 {
        (self.max.x - self.min.x + 1).max(0) as u32
    }
    /// Height along Y (always positive).
    pub fn height(&self) -> u32 {
        (self.max.y - self.min.y + 1).max(0) as u32
    }
    /// Depth along Z (always positive).
    pub fn depth(&self) -> u32 {
        (self.max.z - self.min.z + 1).max(0) as u32
    }

    /// Total cell count (width × height × depth).
    pub fn volume(&self) -> u64 {
        u64::from(self.width()) * u64::from(self.height()) * u64::from(self.depth())
    }

    /// Smallest bbox containing both `self` and `other`.
    pub fn union(&self, other: &Bbox3) -> Bbox3 {
        Bbox3 {
            min: Pos3::new(
                self.min.x.min(other.min.x),
                self.min.y.min(other.min.y),
                self.min.z.min(other.min.z),
            ),
            max: Pos3::new(
                self.max.x.max(other.max.x),
                self.max.y.max(other.max.y),
                self.max.z.max(other.max.z),
            ),
        }
    }
}
