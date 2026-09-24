//! A delay of a whole number of samples.
//!
//! The buffer is sized once for the longest delay it will be asked for, so
//! the length can be changed from the audio thread without allocating.

pub struct Delay {
    buf: Vec<f64>,
    /// Samples of delay. Zero passes the input straight through.
    len: usize,
    pos: usize,
}

impl Delay {
    /// A delay of `len` samples that can later be set to anything up to
    /// `capacity`.
    pub fn new(len: usize, capacity: usize) -> Self {
        let capacity = capacity.max(len);
        Self {
            buf: vec![0.0; capacity],
            len,
            pos: 0,
        }
    }

    /// Changes the length, and clears what is in flight: samples queued for
    /// one delay come out at the wrong time for another.
    pub fn set_delay(&mut self, len: usize) {
        let len = len.min(self.buf.len());
        if len != self.len {
            self.len = len;
            self.reset();
        }
    }

    pub fn delay(&self) -> usize {
        self.len
    }

    #[inline]
    pub fn process(&mut self, x: f64) -> f64 {
        if self.len == 0 {
            return x;
        }
        let out = self.buf[self.pos];
        self.buf[self.pos] = x;
        self.pos += 1;
        if self.pos == self.len {
            self.pos = 0;
        }
        out
    }

    pub fn reset(&mut self) {
        self.buf.iter_mut().for_each(|v| *v = 0.0);
        self.pos = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::Delay;

    fn impulse_at(delay: &mut Delay) -> Option<usize> {
        (0..64).position(|n| delay.process(if n == 0 { 1.0 } else { 0.0 }) == 1.0)
    }

    #[test]
    fn a_delay_delays_by_exactly_its_length() {
        for len in [0, 1, 5, 32] {
            let mut delay = Delay::new(len, 32);
            assert_eq!(impulse_at(&mut delay), Some(len));
        }
    }

    #[test]
    fn changing_the_length_takes_effect_without_growing() {
        let mut delay = Delay::new(10, 10);
        delay.set_delay(3);
        assert_eq!(impulse_at(&mut delay), Some(3));
        delay.set_delay(0);
        assert_eq!(impulse_at(&mut delay), Some(0));
        // Past the capacity it stops at the capacity rather than reallocate.
        delay.set_delay(40);
        assert_eq!(delay.delay(), 10);
        assert_eq!(impulse_at(&mut delay), Some(10));
    }
}
