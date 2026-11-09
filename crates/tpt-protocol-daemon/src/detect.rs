//! Protocol auto-detection: runs every protocol's framer over a capture and
//! ranks the protocols by how much of the capture they decode.
//!
//! This is a heuristic, not a proof. A random byte string can satisfy a weak
//! framer by chance, so the ranking reports the decoded points and frames, and
//! leaves the judgement to the reader. The best answer is usually the one that
//! decodes the most frames with the fewest errors.

use crate::plugins;

/// How one protocol fared on the capture.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Score {
    pub protocol: &'static str,
    /// Frames the protocol's framer accepted.
    pub frames: usize,
    /// Data points those frames produced.
    pub points: usize,
    /// Framing errors (the decoder gave up at that point).
    pub errors: usize,
}

/// Scores every protocol on `bytes`. Protocols that decoded nothing are left out.
/// The result is sorted best first.
pub fn rank(bytes: &[u8]) -> Vec<Score> {
    let mut scores: Vec<Score> = plugins::list_protocols()
        .into_iter()
        .filter_map(|protocol| score(protocol, bytes))
        .collect();
    scores.sort_by(|a, b| {
        b.points
            .cmp(&a.points)
            .then(b.frames.cmp(&a.frames))
            .then(a.errors.cmp(&b.errors))
            .then(a.protocol.cmp(b.protocol))
    });
    scores
}

fn score(protocol: &'static str, bytes: &[u8]) -> Option<Score> {
    let mut buf = bytes.to_vec();
    let mut result = Score {
        protocol,
        frames: 0,
        points: 0,
        errors: 0,
    };
    loop {
        match plugins::decode_one(protocol, &buf) {
            Ok(None) => break,
            Ok(Some((points, used))) => {
                if used == 0 {
                    break; // a framer that consumes nothing would loop forever
                }
                result.frames += 1;
                if let Ok(points) = points {
                    result.points += points.len();
                }
                buf.drain(..used);
                if buf.is_empty() {
                    break;
                }
            }
            Err(_) => {
                result.errors += 1;
                break;
            }
        }
    }
    (result.frames > 0 || result.points > 0).then_some(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modbus_write_frames_rank_modbus_first() {
        let hex = include_str!("../samples/modbus-write-single-register-1.hex");
        let bytes = crate::decode_hex_file(hex.as_bytes()).unwrap();
        let ranking = rank(&[bytes.clone(), bytes].concat());
        assert_eq!(ranking.first().map(|s| s.protocol), Some("modbus"));
        assert_eq!(ranking[0].frames, 2);
        assert_eq!(ranking[0].points, 2);
    }

    #[test]
    fn a_single_junk_byte_scores_at_most_one_point() {
        // UDS has no length field, so any single byte can be a whole message. The
        // ranking therefore shows a one-point hit for junk: a weak score, not a match.
        assert!(rank(&[0xFF]).iter().all(|s| s.points <= 1));
    }
}
