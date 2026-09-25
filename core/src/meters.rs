//! What the front panel meter is fed.
//!
//! The audio thread adds to it every block and the editor takes what has
//! built up each time it draws. Reading only the latest block, which is what
//! this used to do, showed the editor one block in a dozen at a typical buffer
//! size and threw the rest away, so a peak between two frames never reached
//! the needle.

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

/// A sine's peak sits this far above its RMS. The output reading is an
/// average, but it is quoted referred to a sine, so a tone peaking at -18
/// dBFS reads -18 dBFS and puts the needle on 0 VU at the +4 setting, the
/// way the alignment tone does on a digital peak meter.
const SINE_CREST_DB: f32 = 3.0103;

/// The gain reduction meter can read below zero -- all-button mode leaves the
/// gate resting past the point the meter was zeroed at -- so readings are
/// stored this far up, which keeps them positive for `fetch_max`.
const REDUCTION_OFFSET_DB: f32 = 100.0;

/// Past this many samples an unread average is started again rather than
/// added to. An editor that is closed never takes anything, and an average
/// over the last hour is no use to the one that opens next. Also keeps the
/// count well clear of wrapping.
const MOST_SAMPLES: u32 = 1 << 20;

/// What the meter reads for the stretch of audio since the last reading.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Reading {
    /// The highest the gain reduction meter read, in dB. Negative when the
    /// needle is resting past zero.
    pub reduction_db: f32,
    /// Output level in dBFS, from the average power and referred to a sine.
    pub output_db: f32,
}

impl Reading {
    /// No signal at all, which is where the needle rests.
    pub const SILENT: Reading = Reading {
        reduction_db: 0.0,
        output_db: -120.0,
    };
}

#[derive(Default)]
pub struct Meters {
    /// Highest gain reduction reading since the editor last took one, in
    /// hundredths of a dB, raised by [`REDUCTION_OFFSET_DB`].
    reduction: AtomicU32,
    /// Output energy since the editor last took a reading: an `f32` sum of
    /// squares in the high half and the number of samples it covers in the
    /// low half, packed together so the two are always read as a pair.
    output: AtomicU64,
}

impl Meters {
    /// Called from the audio thread once per block.
    ///
    /// `sum_squares` is the output energy of the loudest channel over the
    /// block and `samples` how many samples that is per channel.
    pub fn publish(&self, reduction_db: f32, sum_squares: f32, samples: u32) {
        if samples == 0 {
            return;
        }
        self.reduction.fetch_max(
            ((reduction_db + REDUCTION_OFFSET_DB).max(0.0) * 100.0) as u32,
            Ordering::Relaxed,
        );
        let mut current = self.output.load(Ordering::Relaxed);
        loop {
            let (sum, count) = unpack(current);
            let next = if count >= MOST_SAMPLES {
                pack(sum_squares, samples)
            } else {
                pack(sum + sum_squares, count + samples)
            };
            match self.output.compare_exchange_weak(
                current,
                next,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(actual) => current = actual,
            }
        }
    }

    /// Called from the editor: everything since the last call, or `None` if
    /// no audio has been processed since. A frame that lands between two
    /// blocks gets `None` rather than silence, so a large buffer does not make
    /// the needle drop on every frame the audio has not caught up with.
    pub fn take(&self) -> Option<Reading> {
        let (sum, count) = unpack(self.output.swap(0, Ordering::Relaxed));
        if count == 0 {
            return None;
        }
        let reduction =
            self.reduction.swap(0, Ordering::Relaxed) as f32 / 100.0 - REDUCTION_OFFSET_DB;
        let mean = sum / count as f32;
        Some(Reading {
            reduction_db: reduction,
            output_db: (10.0 * (mean + 1e-12).log10() + SINE_CREST_DB)
                .max(Reading::SILENT.output_db),
        })
    }

    /// Forget everything, as after the host resets the plugin.
    pub fn clear(&self) {
        self.reduction.store(0, Ordering::Relaxed);
        self.output.store(0, Ordering::Relaxed);
    }
}

fn pack(sum: f32, count: u32) -> u64 {
    ((sum.to_bits() as u64) << 32) | count as u64
}

fn unpack(packed: u64) -> (f32, u32) {
    (f32::from_bits((packed >> 32) as u32), packed as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_processed_is_no_reading_rather_than_silence() {
        let meters = Meters::default();
        assert_eq!(meters.take(), None);
        meters.publish(3.0, 1.0, 10);
        assert!(meters.take().is_some());
        assert_eq!(meters.take(), None, "a reading is only taken once");
    }

    /// Several blocks between two frames: the deepest reduction among them,
    /// not the last block's.
    #[test]
    fn a_peak_between_two_frames_is_not_lost() {
        let meters = Meters::default();
        meters.publish(2.0, 0.0, 64);
        meters.publish(9.5, 0.0, 64);
        meters.publish(1.0, 0.0, 64);
        let reading = meters.take().unwrap();
        assert!((reading.reduction_db - 9.5).abs() < 0.011);
    }

    /// Referred to a sine: one peaking at -18 dBFS reads -18 dBFS.
    #[test]
    fn a_sine_reads_its_peak_level() {
        let meters = Meters::default();
        let amplitude = 10f32.powf(-18.0 / 20.0);
        let n = 4800;
        let sum: f32 = (0..n)
            .map(|i| (amplitude * (std::f32::consts::TAU * i as f32 / 48.0).sin()).powi(2))
            .sum();
        meters.publish(0.0, sum, n);
        let reading = meters.take().unwrap();
        assert!(
            (reading.output_db + 18.0).abs() < 0.05,
            "read {:.2} dBFS",
            reading.output_db
        );
    }

    /// All-button mode leaves the needle resting past zero, and the reading
    /// has to survive being stored.
    #[test]
    fn a_reading_below_zero_survives() {
        let meters = Meters::default();
        meters.publish(-30.0, 1.0, 10);
        let reading = meters.take().unwrap();
        assert!(
            (reading.reduction_db + 30.0).abs() < 0.011,
            "read {}",
            reading.reduction_db
        );
        meters.publish(-30.0, 1.0, 10);
        meters.publish(4.0, 1.0, 10);
        assert!((meters.take().unwrap().reduction_db - 4.0).abs() < 0.011);
    }

    /// An unread meter starts its average again instead of holding on to
    /// everything since the editor was last open.
    #[test]
    fn an_unread_average_starts_again() {
        let meters = Meters::default();
        meters.publish(0.0, 1.0e6, MOST_SAMPLES);
        meters.publish(0.0, 0.5, 1);
        let reading = meters.take().unwrap();
        assert!(
            (reading.output_db - (10.0 * 0.5f32.log10() + SINE_CREST_DB)).abs() < 0.01,
            "read {:.2}",
            reading.output_db
        );
    }
}
