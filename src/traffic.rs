use crate::{TileId, TrafficTile, ffi};

/// Real-time traffic data for a single edge, including speeds, congestion levels, and incidents.
/// It is a Rust representation of `valhalla::baldr::TrafficSpeed`.
///
/// A `LiveTraffic` value is a *snapshot* - one volatile read of the record's 8 bytes - so all
/// accessors decode the same coherent bits even while a traffic updater rewrites the tile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LiveTraffic(u64);

impl LiveTraffic {
    /// Live traffic data is unknown for the edge.
    pub const UNKNOWN: Self = Self(0);
    /// Edge is closed due to incident.
    pub const CLOSED: Self = Self(255u64 << 28); // set breakpoint1 to 255, keeping overall_encoded_speed at 0

    /// 7-bit speed value signaling "unknown". Mirrors `UNKNOWN_TRAFFIC_SPEED_RAW` in `traffictile.h`.
    const UNKNOWN_SPEED: u8 = (1 << 7) - 1;
    /// Max raw 6-bit congestion; `1..=63` maps to `[0.0, 1.0]`, `0` = unknown. Mirrors `MAX_CONGESTION_VAL` in `traffictile.h`.
    const MAX_CONGESTION: u8 = 63;

    /// Constructs a `LiveTraffic` instance from its raw `u64` bit representation.
    /// The bit layout of the `u64` value must match the format of the
    /// [`valhalla::baldr::TrafficSpeed`] struct in the C++ Valhalla library.
    ///
    /// [`valhalla::baldr::TrafficSpeed`]: https://github.com/valhalla/valhalla/blob/master/valhalla/baldr/traffictile.h
    #[inline(always)]
    pub const fn from_bits(value: u64) -> Self {
        Self(value)
    }

    /// Returns the raw `u64` bit representation of the traffic data.
    /// The bit layout of the returned value is defined by the
    /// [`valhalla::baldr::TrafficSpeed`] struct in the C++ Valhalla library.
    ///
    /// [`valhalla::baldr::TrafficSpeed`]: https://github.com/valhalla/valhalla/blob/master/valhalla/baldr/traffictile.h
    #[inline(always)]
    pub const fn to_bits(&self) -> u64 {
        self.0
    }

    /// Creates traffic data from a single, uniform speed for the entire edge.
    /// Underlying segmented speeds are set to `[speed, 0, 0]` with breakpoints as `[255, 0]`.
    /// Speeds floor to 2 km/h resolution; `speed >= 254` encodes the unknown sentinel (reads back `None`).
    #[inline(always)]
    pub const fn from_uniform_speed(speed: u8) -> Self {
        Self::from_segmented_speeds(speed, [speed, 0, 0], [255, 0])
    }

    /// Creates traffic data from multiple speed values for different segments of the edge.
    /// Speeds floor to 2 km/h (`>= 254` encodes the unknown sentinel). Breakpoints are `bp/255`
    /// fence fractions: `breakpoints[0] == 255` = uniform; `breakpoints[1] <= breakpoints[0]` truncates coverage.
    #[inline(always)]
    pub const fn from_segmented_speeds(
        overall_speed: u8,
        subsegment_speeds: [u8; 3],
        breakpoints: [u8; 2],
    ) -> Self {
        let overall_encoded = (overall_speed >> 1) as u64;
        let speed1_encoded = (subsegment_speeds[0] >> 1) as u64;
        let speed2_encoded = (subsegment_speeds[1] >> 1) as u64;
        let speed3_encoded = (subsegment_speeds[2] >> 1) as u64;
        let bp1 = breakpoints[0] as u64;
        let bp2 = breakpoints[1] as u64;

        Self(
            overall_encoded |        // overall_encoded_speed at bit 0
            (speed1_encoded << 7) |  // encoded_speed1 at bit 7
            (speed2_encoded << 14) | // encoded_speed2 at bit 14
            (speed3_encoded << 21) | // encoded_speed3 at bit 21
            (bp1 << 28) |            // breakpoint1 at bit 28
            (bp2 << 36), // breakpoint2 at bit 36
        )
    }

    /// Sets the spare bit to the given value, returning a new `LiveTraffic` instance.
    #[inline(always)]
    pub const fn with_spare(self, spare: bool) -> Self {
        let cleared = self.0 & !(1u64 << 63); // Clear the spare bit
        let spare_bit = (spare as u64) << 63;
        Self(cleared | spare_bit)
    }

    /// Gets the value of the spare bit.
    #[inline(always)]
    pub const fn spare(&self) -> bool {
        self.0 & (1 << 63) != 0
    }

    /// Overall speed of the edge in km/h - the value Valhalla costing consumes; see
    /// [`LiveTraffic::segments()`] for per-segment detail. `None` = no live reading; `Some(0)` = closed.
    /// Guard `kph > 0` before dividing or threshold-comparing - closed is not "slow".
    ///
    /// ```
    /// use valhalla::LiveTraffic;
    /// assert_eq!(LiveTraffic::from_uniform_speed(72).speed(), Some(72));
    /// assert_eq!(LiveTraffic::UNKNOWN.speed(), None);
    /// ```
    #[inline(always)]
    pub const fn speed(&self) -> Option<u8> {
        let overall_encoded = (self.0 & 0x7F) as u8; // bits 0..=6
        let breakpoint1 = ((self.0 >> 28) & 0xFF) as u8; // bits 28..=35
        if breakpoint1 == 0 || overall_encoded == Self::UNKNOWN_SPEED {
            None
        } else {
            Some(overall_encoded << 1)
        }
    }

    /// Live-traffic detail as 1-3 contiguous [`TrafficSegment`]s in edge direction; a uniform record
    /// yields one segment `(0.0, 1.0)`. Empty when [`LiveTraffic::speed()`] is `None`; coverage may
    /// end before the edge does.
    #[inline(always)]
    pub fn segments(&self) -> TrafficSegments {
        TrafficSegments {
            bits: self.0,
            index: 0,
        }
    }

    /// Whether the edge references incidents in the corresponding incident tile.
    /// Meaningful even without a speed reading.
    #[inline(always)]
    pub const fn has_incidents(&self) -> bool {
        self.0 & (1 << 62) != 0
    }

    /// Sets the per-segment congestion (`[0.0, 1.0]` clamped, `None` = unknown), preserving all other
    /// bits; read back via [`TrafficSegment::congestion`]. Note: `Some(1.0)` is the wire's *closed*
    /// marker - that segment reads back with `speed == Some(0)` (unless its speed is unknown - `None` wins).
    #[inline(always)]
    pub fn with_congestion(self, congestion: [Option<f32>; 3]) -> Self {
        // Clear the three 6-bit congestion fields (bits 44..=61), preserving everything else.
        let cleared = self.0 & !(0x3_FFFFu64 << 44);
        let c1 = encode_congestion(congestion[0]);
        let c2 = encode_congestion(congestion[1]);
        let c3 = encode_congestion(congestion[2]);
        Self(cleared | (c1 << 44) | (c2 << 50) | (c3 << 56))
    }

    /// Sets or clears the incidents bit, preserving all other bits. Referencing a real incident
    /// tile is the caller's responsibility.
    #[inline(always)]
    pub const fn with_incidents(self, has_incidents: bool) -> Self {
        let cleared = self.0 & !(1u64 << 62); // Clear the incidents bit
        let incidents_bit = (has_incidents as u64) << 62;
        Self(cleared | incidents_bit)
    }
}

/// A contiguous portion of an edge with its own live-traffic reading. Yielded by
/// [`LiveTraffic::segments()`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TrafficSegment {
    /// Portion of the edge this segment covers, as fractions of edge length `(start, end)`, `0.0..=1.0`.
    pub range: (f32, f32),
    /// Speed in km/h; `None` = no data for this portion (wins over congestion-closed), `Some(0)` =
    /// closed (also from congestion `1.0`). Guard `kph > 0` before dividing - closed is not "slow".
    pub speed: Option<u8>,
    /// Congestion level `0.0..=1.0`; `None` = unknown. `1.0` also folds `speed` to `Some(0)` (closed).
    pub congestion: Option<f32>,
}

/// Iterator over the 1-3 [`TrafficSegment`]s of a [`LiveTraffic`] record.
/// Decodes lazily from a copied `u64` - no allocation, detached from the memory-mapped tile.
#[derive(Clone, Debug)]
pub struct TrafficSegments {
    /// Raw record bits, copied at [`LiveTraffic::segments()`] time.
    bits: u64,
    /// Next segment index to yield.
    index: u8,
}

impl Iterator for TrafficSegments {
    type Item = TrafficSegment;

    fn next(&mut self) -> Option<TrafficSegment> {
        let segment = decode_segment(self.bits, self.index)?;
        self.index += 1;
        Some(segment)
    }
}

impl std::iter::FusedIterator for TrafficSegments {}

/// Decodes segment `i` of a record, or `None` when it does not exist - the single place that knows
/// the segment layout. Segment 0 exists iff the record has a reading; segments exist only while the
/// breakpoint fences strictly advance, which uniformly handles uniform records (`breakpoint1 == 255`),
/// truncated coverage (`breakpoint2 <= breakpoint1`), and garbage encodings.
fn decode_segment(bits: u64, i: u8) -> Option<TrafficSegment> {
    if i >= 3 || LiveTraffic::from_bits(bits).speed().is_none() {
        return None;
    }
    let breakpoint1 = ((bits >> 28) & 0xFF) as u8; // bits 28..=35
    let breakpoint2 = ((bits >> 36) & 0xFF) as u8; // bits 36..=43
    let fences = [
        (0, breakpoint1),
        (breakpoint1, breakpoint2),
        (breakpoint2, 255),
    ];
    if fences[..=i as usize]
        .iter()
        .any(|(start, end)| end <= start)
    {
        return None;
    }
    let (start, end) = fences[i as usize];
    let speed_raw = ((bits >> (7 + 7 * i)) & 0x7F) as u8; // encoded_speed{1,2,3}
    let congestion_raw = ((bits >> (44 + 6 * i)) & 0x3F) as u8; // congestion{1,2,3}
    Some(TrafficSegment {
        range: (start as f32 / 255.0, end as f32 / 255.0),
        speed: decode_segment_speed(speed_raw, congestion_raw),
        congestion: decode_congestion(congestion_raw as u64),
    })
}

/// Decodes a segment's raw 7-bit speed and raw 6-bit congestion pair into its reading:
/// - `None` when the speed holds the UNKNOWN sentinel (`127`) - partial coverage. Unknown speed
///   wins over congestion-closed (unlike C++ `closed(subsegment)`, which reports sentinel + raw-63 as closed).
/// - `Some(0)` (closed) when the congestion is raw `63` - the C++ `closed(subsegment)` semantic.
/// - `Some(speed_raw << 1)` otherwise - a raw speed of `0` yields `Some(0)` (closed) naturally,
///   the wire's own encoding of closure.
#[inline(always)]
const fn decode_segment_speed(speed_raw: u8, congestion_raw: u8) -> Option<u8> {
    if speed_raw == LiveTraffic::UNKNOWN_SPEED {
        None
    } else if congestion_raw == LiveTraffic::MAX_CONGESTION {
        Some(0)
    } else {
        Some(speed_raw << 1)
    }
}

/// Decodes a 6-bit raw congestion value into a normalized `[0.0, 1.0]` fraction, or `None` when
/// unknown (raw `0`).
#[inline(always)]
fn decode_congestion(raw: u64) -> Option<f32> {
    if raw == 0 {
        None
    } else {
        Some((raw as f32 - 1.0) / (LiveTraffic::MAX_CONGESTION as f32 - 1.0))
    }
}

/// Encodes a normalized `[0.0, 1.0]` congestion fraction into its 6-bit raw value, the inverse of
/// [`decode_congestion`]. `None` and non-finite values map to raw `0` (unknown); finite `Some(f)`
/// clamps to `[0.0, 1.0]` and maps to `round(f * 62) + 1` in `1..=63`.
#[inline(always)]
fn encode_congestion(congestion: Option<f32>) -> u64 {
    match congestion {
        Some(f) if f.is_finite() => {
            (f.clamp(0.0, 1.0) * (LiveTraffic::MAX_CONGESTION as f32 - 1.0)).round() as u64 + 1
        }
        _ => 0,
    }
}

impl TrafficTile {
    /// Id of the graph tile this traffic tile belongs to.
    #[inline(always)]
    pub fn id(&self) -> TileId {
        ffi::id(self).tile()
    }

    /// Seconds since epoch of the last update.
    #[inline(always)]
    pub fn last_update(&self) -> u64 {
        ffi::last_update(self)
    }

    /// Writes the last update timestamp to the memory-mapped file.
    #[inline(always)]
    pub fn write_last_update(&self, unix_timestamp: u64) {
        ffi::write_last_update(self, unix_timestamp)
    }

    /// Custom spare value stored in the header.
    #[inline(always)]
    pub fn spare(&self) -> u64 {
        ffi::spare(self)
    }

    /// Writes a custom value to the spare field in the memory-mapped file.
    #[inline(always)]
    pub fn write_spare(&self, spare: u64) {
        ffi::write_spare(self, spare)
    }

    /// Number of directed edges in this traffic tile.
    #[inline(always)]
    pub fn edge_count(&self) -> u32 {
        self.edge_count
    }

    /// Live traffic information for the given edge index in the tile if available.
    #[inline(always)]
    pub fn edge_traffic(&self, edge_index: u32) -> Option<LiveTraffic> {
        if edge_index < self.edge_count {
            let data = unsafe { std::ptr::read_volatile(self.speeds.add(edge_index as usize)) };
            Some(LiveTraffic(data))
        } else {
            None
        }
    }

    /// Writes live traffic information for the given edge index in the tile.
    #[inline(always)]
    pub fn write_edge_traffic(&self, edge_index: u32, traffic: LiveTraffic) {
        if edge_index < self.edge_count {
            unsafe { std::ptr::write_volatile(self.speeds.add(edge_index as usize), traffic.0) };
        }
    }

    /// Clears live traffic information in the tile and sets the last update time to 0.
    /// The spare field is left unchanged.
    pub fn clear_traffic(&self) {
        for i in 0..self.edge_count as usize {
            unsafe { std::ptr::write_volatile(self.speeds.add(i), 0u64) };
        }
        self.write_last_update(0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_live_traffic_tile() {
        let mut header: [u64; 16] = [0; 16]; // it should be just big enough. The exact header size is 32 bytes.
        let mut speeds: [u64; 16] = [0; 16];
        let tile = TrafficTile {
            header: header.as_mut_ptr(),
            speeds: speeds.as_mut_ptr(),
            edge_count: 16,
            traffic_tar: cxx::SharedPtr::null(),
        };

        // Initial state with all zeros
        assert_eq!(tile.last_update(), 0);
        assert_eq!(tile.spare(), 0);
        assert_eq!(tile.edge_count(), 16);
        for i in 0..tile.edge_count() {
            assert_eq!(tile.edge_traffic(i), Some(LiveTraffic::UNKNOWN));
        }

        // Out-of-range access
        assert_eq!(tile.edge_traffic(tile.edge_count()), None);
        assert_eq!(tile.edge_traffic(u32::MAX), None);
        let before = speeds;
        tile.write_edge_traffic(tile.edge_count(), LiveTraffic::from_uniform_speed(200));
        tile.write_edge_traffic(u32::MAX, LiveTraffic::from_uniform_speed(200));
        assert_eq!(speeds, before, "out-of-range write must be a no-op");

        // Let's mutate stuff
        tile.write_last_update(1234567890);
        tile.write_spare(42);
        for i in 0..tile.edge_count() {
            tile.write_edge_traffic(i, LiveTraffic::from_uniform_speed(i as u8));
        }

        assert_eq!(tile.last_update(), 1234567890);
        assert_eq!(tile.spare(), 42);
        for i in 0..tile.edge_count() {
            assert_eq!(
                tile.edge_traffic(i),
                Some(LiveTraffic::from_uniform_speed(i as u8))
            );
        }

        // Each speed is just a u64, encoded in a specific way. This checks that there is no weirdness in that area.
        for i in 0..tile.edge_count() {
            assert_eq!(
                speeds[i as usize],
                LiveTraffic::from_uniform_speed(i as u8).to_bits()
            );
        }

        // The high bits (congestion 44..=61, incidents 62, spare 63) must survive the raw volatile
        // round-trip too, not just the low 44 bits written by `from_uniform_speed`.
        let full = LiveTraffic::from_segmented_speeds(72, [60, 80, 100], [127, 200])
            .with_congestion([Some(0.5), None, Some(1.0)])
            .with_incidents(true)
            .with_spare(true);
        tile.write_edge_traffic(3, full);
        assert_eq!(tile.edge_traffic(3), Some(full));
        assert_eq!(speeds[3], full.to_bits());
        // Setting incidents (62) and spare (63) means the record occupies the high half of the u64.
        assert!(full.has_incidents());
        assert!(full.spare());
        assert_ne!(full.to_bits() >> 32, 0, "high 32 bits must be exercised");
    }

    #[test]
    fn live_traffic_speed() {
        use pretty_assertions::assert_eq;

        // UNKNOWN sentinel record carries no reading (breakpoint1 == 0).
        assert_eq!(LiveTraffic::UNKNOWN.speed(), None);
        // CLOSED record: breakpoint1 == 255, overall_encoded == 0 -> Some(0), the wire's own
        // encoding of closure.
        assert_eq!(LiveTraffic::CLOSED.speed(), Some(0));

        // Uniform known speed.
        assert_eq!(LiveTraffic::from_uniform_speed(72).speed(), Some(72));

        // Segmented known speed uses the overall speed only.
        assert_eq!(
            LiveTraffic::from_segmented_speeds(72, [60, 80, 100], [127, 200]).speed(),
            Some(72)
        );

        // The C++ INVALID_SPEED record (overall + all three subsegment speeds == 127,
        // breakpoints == 0) carries no reading.
        assert_eq!(LiveTraffic::from_bits(0x0FFF_FFFF).speed(), None);

        // The 127 sentinel gates on its own: no reading even with a nonzero breakpoint.
        let invalid = LiveTraffic::from_bits(
            (LiveTraffic::UNKNOWN_SPEED as u64) // overall_encoded == 127
            | (255u64 << 28), // breakpoint1 != 0
        );
        assert_eq!(invalid.speed(), None);

        // Precedence: `breakpoint1 == 0` gates first. An all-zero record has no reading - it is
        // NOT closed (`Some(0)`), even though its overall_encoded is 0.
        assert_eq!(LiveTraffic::from_bits(0).speed(), None);
        // A nonzero overall speed with `breakpoint1 == 0` still has no reading (no valid coverage).
        let no_breakpoint = LiveTraffic::from_bits(36); // overall_encoded == 36, breakpoint1 == 0
        assert_eq!(no_breakpoint.speed(), None);
    }

    #[test]
    fn live_traffic_segments() {
        use pretty_assertions::assert_eq;

        let segment = |range, speed, congestion| TrafficSegment {
            range,
            speed,
            congestion,
        };

        // No reading -> no segments (UNKNOWN and the C++ INVALID_SPEED record alike).
        assert_eq!(LiveTraffic::UNKNOWN.segments().count(), 0);
        assert_eq!(LiveTraffic::from_bits(0x0FFF_FFFF).segments().count(), 0);

        // CLOSED and uniform records are not special cases - each is exactly one segment
        // covering the whole edge.
        assert_eq!(
            LiveTraffic::CLOSED.segments().collect::<Vec<_>>(),
            [segment((0.0, 1.0), Some(0), None)]
        );
        assert_eq!(
            LiveTraffic::from_uniform_speed(72)
                .segments()
                .collect::<Vec<_>>(),
            [segment((0.0, 1.0), Some(72), None)]
        );

        // Segmented record exercising all three per-segment states: moving, closed (speed 0),
        // and no-data (254 kph in -> the encoded 127 UNKNOWN sentinel).
        let (bp1, bp2) = (100.0 / 255.0, 200.0 / 255.0);
        assert_eq!(
            LiveTraffic::from_segmented_speeds(72, [50, 0, 254], [100, 200])
                .segments()
                .collect::<Vec<_>>(),
            [
                segment((0.0, bp1), Some(50), None),
                segment((bp1, bp2), Some(0), None),
                segment((bp2, 1.0), None, None),
            ]
        );

        // All three subsegments unknown while the overall speed is known: the overall summary
        // stays authoritative.
        let all_unknown = LiveTraffic::from_segmented_speeds(72, [254, 254, 254], [100, 200]);
        assert_eq!(all_unknown.speed(), Some(72));
        assert!(
            all_unknown
                .segments()
                .all(|segment| segment.speed.is_none())
        );
        assert_eq!(all_unknown.segments().count(), 3);

        // Fence boundaries: breakpoint2 == 255 ends the record at two segments; breakpoint2 <=
        // breakpoint1 (0, equal, or garbage out-of-order) truncates coverage to a single segment.
        let two = LiveTraffic::from_segmented_speeds(72, [60, 80, 100], [127, 255]);
        assert_eq!(
            two.segments().collect::<Vec<_>>(),
            [
                segment((0.0, 127.0 / 255.0), Some(60), None),
                segment((127.0 / 255.0, 1.0), Some(80), None),
            ]
        );
        for breakpoints in [[127, 0], [100, 80], [100, 100]] {
            let truncated = LiveTraffic::from_segmented_speeds(72, [60, 80, 100], breakpoints);
            assert_eq!(
                truncated.segments().collect::<Vec<_>>(),
                [segment(
                    (0.0, breakpoints[0] as f32 / 255.0),
                    Some(60),
                    None
                )],
                "breakpoints {breakpoints:?}"
            );
        }
        assert_eq!(
            LiveTraffic::from_segmented_speeds(72, [60, 80, 100], [255, 255])
                .segments()
                .collect::<Vec<_>>(),
            [segment((0.0, 1.0), Some(60), None)]
        );

        // Congestion: raw 0 / 1 / 63 -> None / Some(0.0) / Some(1.0) per 6-bit field, and raw 63
        // (== written 1.0) also folds that segment's speed to Some(0) = closed (C++
        // `closed(subsegment)`) - unless the segment's speed is unknown, which wins.
        let bits = LiveTraffic::from_segmented_speeds(72, [60, 80, 100], [100, 200]).to_bits();
        let record = LiveTraffic::from_bits(
            bits | (1u64 << 44) | ((LiveTraffic::MAX_CONGESTION as u64) << 56),
        );
        assert_eq!(
            record.segments().collect::<Vec<_>>(),
            [
                segment((0.0, bp1), Some(60), Some(0.0)),
                segment((bp1, bp2), Some(80), None),
                segment((bp2, 1.0), Some(0), Some(1.0)),
            ]
        );
        let sentinel_and_congested =
            LiveTraffic::from_segmented_speeds(72, [254, 50, 50], [100, 200]).with_congestion([
                Some(1.0),
                None,
                None,
            ]);
        assert_eq!(
            sentinel_and_congested.segments().next().unwrap(),
            segment((0.0, bp1), None, Some(1.0))
        );

        // with_congestion round-trip: quantized to 6 bits (step 1/62), so only exactly-representable
        // values (endpoints and 0.5) survive `==`; out-of-range clamps; non-finite is not a reading.
        let base = LiveTraffic::from_segmented_speeds(72, [60, 80, 100], [100, 200]);
        let congestion = |t: LiveTraffic| {
            t.segments()
                .map(|segment| segment.congestion)
                .collect::<Vec<_>>()
        };
        let with = base.with_congestion([None, Some(0.0), Some(0.5)]);
        assert_eq!(congestion(with), [None, Some(0.0), Some(0.5)]);
        let clamped = base.with_congestion([Some(-1.0), Some(2.0), None]);
        assert_eq!(congestion(clamped), [Some(0.0), Some(1.0), None]);
        let non_finite =
            base.with_congestion([Some(f32::NAN), Some(f32::INFINITY), Some(f32::NEG_INFINITY)]);
        assert_eq!(congestion(non_finite), [None, None, None]);

        // with_congestion only touches the congestion bits (44..=61): speed, ranges, incidents and
        // spare are preserved, and clearing back to all-None restores the original bits exactly.
        let record = base.with_incidents(true).with_spare(true);
        let with = record.with_congestion([Some(0.25), Some(0.75), None]);
        assert_eq!(with.speed(), record.speed());
        for (w, r) in with.segments().zip(record.segments()) {
            assert_eq!(w.range, r.range);
            assert_eq!(w.speed, r.speed);
        }
        assert!(with.has_incidents() && with.spare());
        assert_eq!(with.with_congestion([None, None, None]), record);
    }

    #[test]
    fn live_traffic_incidents() {
        use pretty_assertions::assert_eq;

        // Bit 62 reads via has_incidents(); the spare bit (63) must not be mistaken for it.
        assert!(!LiveTraffic::UNKNOWN.has_incidents());
        assert!(LiveTraffic::from_bits(1u64 << 62).has_incidents());
        assert!(!LiveTraffic::UNKNOWN.with_spare(true).has_incidents());

        let record = LiveTraffic::from_uniform_speed(72)
            .with_congestion([Some(0.5), None, Some(1.0)])
            .with_spare(true);

        let set = record.with_incidents(true);
        assert!(set.has_incidents());
        // Speed, segments (including their congestion) and spare are untouched.
        assert_eq!(set.speed(), record.speed());
        assert_eq!(
            set.segments().collect::<Vec<_>>(),
            record.segments().collect::<Vec<_>>()
        );
        assert!(set.spare());

        // Clearing the incidents bit restores the original record exactly.
        let cleared = set.with_incidents(false);
        assert!(!cleared.has_incidents());
        assert_eq!(cleared, record);
    }
}
