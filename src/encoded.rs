//! Decoders for the arrays packed inside a tile, borrowing it and allocating nothing.
//! Mirrors `midgard/encoded.h` and `midgard/elevation_encoding.h`.

use crate::LatLon;

/// Reads one varint, advancing `bytes` past it. The 5-chunk bound covers `u32` and keeps a corrupt
/// tile from running away.
#[inline(always)]
fn varint(bytes: &mut &[u8]) -> Option<u32> {
    let mut result = 0u32;
    for i in 0..bytes.len().min(5) {
        let chunk = bytes[i] as u32;
        result |= (chunk & 0x7f) << (i * 7); // no shift overflow as i < 5
        if chunk & 0x80 == 0 {
            *bytes = &bytes[i + 1..];
            return Some(result);
        }
    }
    None
}

/// Restores the sign bit that [zigzag encoding] moved to the least significant bit.
///
/// [zigzag encoding]: https://protobuf.dev/programming-guides/encoding/#signed-ints
#[inline(always)]
fn zigzag_decode(value: u32) -> i32 {
    (value >> 1) as i32 ^ -((value & 1) as i32)
}

/// Counts the varints in `bytes` - every byte without the continuation bit ends one.
#[inline(always)]
fn varint_count(bytes: &[u8]) -> usize {
    bytes.iter().filter(|&&byte| byte & 0x80 == 0).count()
}

/// Points of an edge shape: zigzag varint deltas at 1e-6 degrees, lat and lon as separate varints.
///
/// Stored order is the way's direction, not the edge's - see [`crate::DirectedEdge::forward()`].
#[derive(Clone)]
pub struct Shape<'a> {
    shape: &'a [u8],
    /// Last decoded coordinate, in 1e-6 degrees.
    lat: i32,
    lon: i32,
}

impl<'a> Shape<'a> {
    #[inline(always)]
    pub fn new(shape: &'a [u8]) -> Self {
        Shape {
            shape,
            lat: 0,
            lon: 0,
        }
    }

    #[inline(always)]
    fn delta(&mut self) -> Option<i32> {
        // A zigzag encoded coordinate stays under 2^29, so `u32` is always enough.
        varint(&mut self.shape).map(zigzag_decode)
    }

    /// Points left, counted without consuming the iterator.
    pub fn len(&self) -> usize {
        varint_count(self.shape) / 2 // two varints per point
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl Iterator for Shape<'_> {
    type Item = LatLon;

    #[inline]
    fn next(&mut self) -> Option<LatLon> {
        self.lat += self.delta()?;
        self.lon += self.delta()?;
        Some(LatLon(self.lat as f64 * 1e-6, self.lon as f64 * 1e-6))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        // Each coordinate takes 1..=5 bytes, and a point is two of them.
        let len = self.shape.len();
        (len / 10, Some(len / 2))
    }

    fn count(self) -> usize {
        self.len()
    }
}

impl std::iter::FusedIterator for Shape<'_> {}

/// Fixed precision elevation is stored in 0.25 meter steps.
const ELEVATION_PRECISION: f32 = 0.25;

/// Elevation *between* an edge's end nodes, in meters: one-byte deltas at 0.25 m, evenly spaced.
///
/// Yielded in travel order - a reverse edge pays a one-off sum rather than a collect.
#[derive(Clone)]
pub struct Elevation<'a> {
    deltas: &'a [i8],
    /// Previously yielded sample, in meters. Seeded with the node the deltas accumulate from.
    front: f32,
    /// The last remaining sample, once someone asks for it. Invariant once set:
    /// `back == front + sum(deltas)`, which `next()` preserves.
    back: Option<f32>,
    /// Whether travel order is the order the deltas are stored in.
    forward: bool,
}

impl<'a> Elevation<'a> {
    #[inline(always)]
    pub(crate) fn new(deltas: &'a [i8], first: f32, forward: bool) -> Self {
        Elevation {
            deltas,
            front: first,
            back: None,
            forward,
        }
    }

    /// One step from the end the deltas accumulate from.
    #[inline]
    fn step_front(&mut self) -> Option<f32> {
        let (&delta, rest) = self.deltas.split_first()?;
        self.deltas = rest;
        self.front += delta as f32 * ELEVATION_PRECISION;
        Some(self.front)
    }

    /// One step from the other end. Only the first pays for the sum.
    #[inline]
    fn step_back(&mut self) -> Option<f32> {
        let (&delta, rest) = self.deltas.split_last()?;
        let back = *self.back.get_or_insert_with(|| {
            let sum: i32 = self.deltas.iter().map(|&d| d as i32).sum();
            self.front + sum as f32 * ELEVATION_PRECISION
        });
        self.deltas = rest;
        self.back = Some(back - delta as f32 * ELEVATION_PRECISION);
        Some(back)
    }

    pub fn len(&self) -> usize {
        self.deltas.len()
    }

    pub fn is_empty(&self) -> bool {
        self.deltas.is_empty()
    }
}

impl Iterator for Elevation<'_> {
    type Item = f32;

    #[inline]
    fn next(&mut self) -> Option<f32> {
        if self.forward {
            self.step_front()
        } else {
            self.step_back()
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.deltas.len(), Some(self.deltas.len()))
    }
}

impl DoubleEndedIterator for Elevation<'_> {
    fn next_back(&mut self) -> Option<f32> {
        if self.forward {
            self.step_back()
        } else {
            self.step_front()
        }
    }
}

impl ExactSizeIterator for Elevation<'_> {}
impl std::iter::FusedIterator for Elevation<'_> {}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    fn encode_varint(value: u32, out: &mut Vec<u8>) {
        let mut value = value;
        while value >= 0x80 {
            out.push((value as u8) | 0x80);
            value >>= 7;
        }
        out.push(value as u8);
    }

    fn zigzag_encode(value: i32) -> u32 {
        ((value << 1) ^ (value >> 31)) as u32
    }

    fn encode_shape(points: &[(f64, f64)]) -> Vec<u8> {
        let (mut lat, mut lon) = (0i32, 0i32);
        let mut out = Vec::new();
        for &(plat, plon) in points {
            let (next_lat, next_lon) = ((plat * 1e6) as i32, (plon * 1e6) as i32);
            encode_varint(zigzag_encode(next_lat - lat), &mut out);
            encode_varint(zigzag_encode(next_lon - lon), &mut out);
            (lat, lon) = (next_lat, next_lon);
        }
        out
    }

    #[test]
    fn shape_round_trip() {
        let points = [
            (42.493898, 1.500757),
            (42.493855, 1.500668),
            (42.493785, 1.500565),
            (-42.5, -1.5), // deltas big enough to need several bytes, and negative
        ];
        let encoded = encode_shape(&points);
        let shape = Shape::new(&encoded);

        assert_eq!(shape.len(), points.len());
        assert_eq!(shape.clone().count(), points.len());
        assert!(!shape.is_empty());
        let (lo, hi) = shape.size_hint();
        assert!(lo <= points.len() && points.len() <= hi.unwrap());

        let decoded: Vec<_> = shape.collect();
        assert_eq!(decoded.len(), points.len());
        for (decoded, &(lat, lon)) in decoded.iter().zip(&points) {
            assert!((decoded.0 - lat).abs() < 1e-9, "{decoded:?} != {lat}");
            assert!((decoded.1 - lon).abs() < 1e-9, "{decoded:?} != {lon}");
        }
    }

    #[test]
    fn shape_handles_empty_and_truncated() {
        let mut empty = Shape::new(&[]);
        assert_eq!(empty.len(), 0);
        assert!(empty.is_empty());
        assert_eq!(empty.next(), None);

        // A varint that never terminates, and a point missing its longitude.
        assert_eq!(Shape::new(&[0x80, 0x80, 0x80]).count(), 0);
        let point = encode_shape(&[(1.0, 2.0)]);
        assert_eq!(Shape::new(&point[..1]).next(), None);
    }

    #[test]
    fn elevation_accumulates_from_the_first_node() {
        // 0.25 m per unit: +4 -> +1.0, -8 -> -2.0
        let deltas = [4i8, -8, 0, 127, -128];
        let elevation = Elevation::new(&deltas, 100.0, true);

        assert_eq!(elevation.len(), deltas.len());
        assert_eq!(
            elevation.collect::<Vec<_>>(),
            vec![101.0, 99.0, 99.0, 130.75, 98.75]
        );
    }

    #[test]
    fn elevation_reads_backwards() {
        let deltas = [4i8, -8, 0, 127, -128];
        let forward: Vec<_> = Elevation::new(&deltas, 100.0, true).collect();
        let backward: Vec<_> = Elevation::new(&deltas, 100.0, true).rev().collect();

        assert_eq!(backward, forward.iter().rev().copied().collect::<Vec<_>>());

        // Both ends of the same iterator meet in the middle without overlapping or skipping.
        let mut both = Elevation::new(&deltas, 100.0, true);
        let mut taken = vec![both.next().unwrap()];
        taken.push(both.next_back().unwrap());
        taken.push(both.next().unwrap());
        taken.push(both.next_back().unwrap());
        taken.push(both.next().unwrap());
        assert_eq!(both.next(), None);
        assert_eq!(both.next_back(), None);
        assert_eq!(
            taken,
            vec![forward[0], forward[4], forward[1], forward[3], forward[2]]
        );
    }

    #[test]
    fn elevation_without_samples_is_empty() {
        let mut elevation = Elevation::new(&[], 42.5, true);
        assert_eq!(elevation.len(), 0);
        assert!(elevation.is_empty());
        assert_eq!(elevation.next(), None);
    }
}
