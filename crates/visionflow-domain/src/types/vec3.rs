//! Simplified Vec3 data type for domain logic
//!
//! This is the domain-layer version of Vec3Data — pure data with serde support.
//! No GPU concerns (bytemuck, repr(C), glam). For GPU-side Vec3Data with Pod/Zeroable,
//! use the main crate's `src/types/vec3.rs`.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Vec3Data {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Vec3Data {
    pub fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

    pub fn zero() -> Self {
        Self::new(0.0, 0.0, 0.0)
    }

    pub fn as_array(&self) -> [f32; 3] {
        [self.x, self.y, self.z]
    }

    /// Euclidean distance to another point
    pub fn distance_to(&self, other: &Vec3Data) -> f32 {
        let dx = self.x - other.x;
        let dy = self.y - other.y;
        let dz = self.z - other.z;
        (dx * dx + dy * dy + dz * dz).sqrt()
    }

    /// Squared distance (avoids sqrt, useful for comparisons)
    pub fn distance_squared_to(&self, other: &Vec3Data) -> f32 {
        let dx = self.x - other.x;
        let dy = self.y - other.y;
        let dz = self.z - other.z;
        dx * dx + dy * dy + dz * dz
    }

    /// Vector length / magnitude
    pub fn length(&self) -> f32 {
        (self.x * self.x + self.y * self.y + self.z * self.z).sqrt()
    }

    /// Squared length (avoids sqrt)
    pub fn length_squared(&self) -> f32 {
        self.x * self.x + self.y * self.y + self.z * self.z
    }

    /// Returns a unit vector in the same direction, or zero if length is zero
    pub fn normalized(&self) -> Self {
        let len = self.length();
        if len < f32::EPSILON {
            Self::zero()
        } else {
            Self::new(self.x / len, self.y / len, self.z / len)
        }
    }

    /// Dot product
    pub fn dot(&self, other: &Vec3Data) -> f32 {
        self.x * other.x + self.y * other.y + self.z * other.z
    }

    /// Cross product
    pub fn cross(&self, other: &Vec3Data) -> Self {
        Self::new(
            self.y * other.z - self.z * other.y,
            self.z * other.x - self.x * other.z,
            self.x * other.y - self.y * other.x,
        )
    }

    /// Linear interpolation between self and other
    pub fn lerp(&self, other: &Vec3Data, t: f32) -> Self {
        Self::new(
            self.x + (other.x - self.x) * t,
            self.y + (other.y - self.y) * t,
            self.z + (other.z - self.z) * t,
        )
    }
}

impl Default for Vec3Data {
    fn default() -> Self {
        Self::zero()
    }
}

impl From<[f32; 3]> for Vec3Data {
    fn from(arr: [f32; 3]) -> Self {
        Self {
            x: arr[0],
            y: arr[1],
            z: arr[2],
        }
    }
}

impl From<Vec3Data> for [f32; 3] {
    fn from(v: Vec3Data) -> Self {
        [v.x, v.y, v.z]
    }
}

impl std::ops::Add for Vec3Data {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self::new(self.x + rhs.x, self.y + rhs.y, self.z + rhs.z)
    }
}

impl std::ops::Sub for Vec3Data {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        Self::new(self.x - rhs.x, self.y - rhs.y, self.z - rhs.z)
    }
}

impl std::ops::Mul<f32> for Vec3Data {
    type Output = Self;
    fn mul(self, rhs: f32) -> Self {
        Self::new(self.x * rhs, self.y * rhs, self.z * rhs)
    }
}

impl std::ops::Neg for Vec3Data {
    type Output = Self;
    fn neg(self) -> Self {
        Self::new(-self.x, -self.y, -self.z)
    }
}

/// Compact binary node data for network transmission (client-facing).
/// Mirrors the layout of `BinaryNodeDataClient` from the main crate
/// but without `bytemuck` derives (domain crate is dependency-light).
#[repr(C)]
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct BinaryNodeData {
    pub node_id: u32,
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub vx: f32,
    pub vy: f32,
    pub vz: f32,
}

impl BinaryNodeData {
    /// Construct from explicit position and velocity Vec3Data.
    pub fn new(node_id: u32, position: Vec3Data, velocity: Vec3Data) -> Self {
        Self {
            node_id,
            x: position.x,
            y: position.y,
            z: position.z,
            vx: velocity.x,
            vy: velocity.y,
            vz: velocity.z,
        }
    }

    /// Returns the position as a Vec3Data.
    pub fn position(&self) -> Vec3Data {
        Vec3Data::new(self.x, self.y, self.z)
    }

    /// Returns the velocity as a Vec3Data.
    pub fn velocity(&self) -> Vec3Data {
        Vec3Data::new(self.vx, self.vy, self.vz)
    }

    /// Default mass for client nodes.
    pub fn mass(&self) -> f32 {
        1.0
    }
}

impl Default for BinaryNodeData {
    fn default() -> Self {
        Self {
            node_id: 0,
            x: 0.0,
            y: 0.0,
            z: 0.0,
            vx: 0.0,
            vy: 0.0,
            vz: 0.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_array_conversion() {
        let array = [1.0, 2.0, 3.0];
        let vec3_data: Vec3Data = array.into();
        let back_to_array: [f32; 3] = vec3_data.into();

        assert_eq!(vec3_data.x, 1.0);
        assert_eq!(vec3_data.y, 2.0);
        assert_eq!(vec3_data.z, 3.0);
        assert_eq!(back_to_array, array);
    }

    #[test]
    fn test_zero() {
        let zero = Vec3Data::zero();
        assert_eq!(zero.x, 0.0);
        assert_eq!(zero.y, 0.0);
        assert_eq!(zero.z, 0.0);
    }

    #[test]
    fn test_distance() {
        let a = Vec3Data::new(0.0, 0.0, 0.0);
        let b = Vec3Data::new(3.0, 4.0, 0.0);
        assert!((a.distance_to(&b) - 5.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_normalized() {
        let v = Vec3Data::new(3.0, 0.0, 0.0);
        let n = v.normalized();
        assert!((n.x - 1.0).abs() < f32::EPSILON);
        assert!(n.y.abs() < f32::EPSILON);

        let zero = Vec3Data::zero().normalized();
        assert_eq!(zero, Vec3Data::zero());
    }

    #[test]
    fn test_dot_cross() {
        let x = Vec3Data::new(1.0, 0.0, 0.0);
        let y = Vec3Data::new(0.0, 1.0, 0.0);
        assert!((x.dot(&y)).abs() < f32::EPSILON);

        let z = x.cross(&y);
        assert!((z.z - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_lerp() {
        let a = Vec3Data::new(0.0, 0.0, 0.0);
        let b = Vec3Data::new(10.0, 10.0, 10.0);
        let mid = a.lerp(&b, 0.5);
        assert!((mid.x - 5.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_ops() {
        let a = Vec3Data::new(1.0, 2.0, 3.0);
        let b = Vec3Data::new(4.0, 5.0, 6.0);

        let sum = a + b;
        assert_eq!(sum, Vec3Data::new(5.0, 7.0, 9.0));

        let diff = b - a;
        assert_eq!(diff, Vec3Data::new(3.0, 3.0, 3.0));

        let scaled = a * 2.0;
        assert_eq!(scaled, Vec3Data::new(2.0, 4.0, 6.0));

        let neg = -a;
        assert_eq!(neg, Vec3Data::new(-1.0, -2.0, -3.0));
    }
}
