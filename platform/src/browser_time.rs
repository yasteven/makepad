
//! Browser clocks through Makepad's existing imports, without wasm-bindgen.
//! Native builds retain the standard-library types.
#[cfg(not(target_arch = "wasm32"))]
pub use std::time::*;

#[cfg(target_arch = "wasm32")]
mod browser {
    pub use std::time::{Duration, TryFromFloatSecsError};
    use std::ops::{Add, AddAssign, Sub, SubAssign};

    // Integer nanoseconds preserve ordering and support std-style budget checks.
    fn nanos(seconds: f64) -> u64 {
        if seconds.is_finite() && seconds > 0.0 {
            (seconds * 1_000_000_000.0) as u64
        } else { 0 }
    }
    fn delta(value: Duration) -> Option<u64> { u64::try_from(value.as_nanos()).ok() }

    #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct Instant(u64);
    impl Instant {
        pub fn now() -> Self { Self(nanos(crate::Cx::monotonic_now())) }
        pub fn elapsed(&self) -> Duration { Self::now().saturating_duration_since(*self) }
        pub fn checked_duration_since(&self, earlier: Self) -> Option<Duration> {
            self.0.checked_sub(earlier.0).map(Duration::from_nanos)
        }
        pub fn saturating_duration_since(&self, earlier: Self) -> Duration {
            self.checked_duration_since(earlier).unwrap_or(Duration::ZERO)
        }
        pub fn duration_since(&self, earlier: Self) -> Duration {
            self.saturating_duration_since(earlier)
        }
        pub fn checked_add(&self, duration: Duration) -> Option<Self> {
            self.0.checked_add(delta(duration)?).map(Self)
        }
        pub fn checked_sub(&self, duration: Duration) -> Option<Self> {
            self.0.checked_sub(delta(duration)?).map(Self)
        }
    }
    impl Add<Duration> for Instant {
        type Output = Self;
        fn add(self, rhs: Duration) -> Self { self.checked_add(rhs).expect("Instant overflow") }
    }
    impl Sub<Duration> for Instant {
        type Output = Self;
        fn sub(self, rhs: Duration) -> Self { self.checked_sub(rhs).expect("Instant underflow") }
    }
    impl Sub<Instant> for Instant {
        type Output = Duration;
        fn sub(self, rhs: Self) -> Duration { self.duration_since(rhs) }
    }
    impl AddAssign<Duration> for Instant { fn add_assign(&mut self, rhs: Duration) { *self = *self + rhs; } }
    impl SubAssign<Duration> for Instant { fn sub_assign(&mut self, rhs: Duration) { *self = *self - rhs; } }

    #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct SystemTime(u64);
    pub const UNIX_EPOCH: SystemTime = SystemTime(0);
    #[derive(Clone, Copy, Debug)]
    pub struct SystemTimeError(Duration);
    impl SystemTimeError { pub fn duration(&self) -> Duration { self.0 } }
    impl std::fmt::Display for SystemTimeError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "second time provided was later than self")
        }
    }
    impl std::error::Error for SystemTimeError {}
    impl SystemTime {
        pub const UNIX_EPOCH: Self = UNIX_EPOCH;
        pub fn now() -> Self { Self(nanos(crate::Cx::time_now())) }
        pub fn elapsed(&self) -> Result<Duration, SystemTimeError> { Self::now().duration_since(*self) }
        pub fn duration_since(&self, earlier: Self) -> Result<Duration, SystemTimeError> {
            if self.0 >= earlier.0 { Ok(Duration::from_nanos(self.0 - earlier.0)) }
            else { Err(SystemTimeError(Duration::from_nanos(earlier.0 - self.0))) }
        }
        pub fn checked_add(&self, duration: Duration) -> Option<Self> { self.0.checked_add(delta(duration)?).map(Self) }
        pub fn checked_sub(&self, duration: Duration) -> Option<Self> { self.0.checked_sub(delta(duration)?).map(Self) }
    }
    impl Add<Duration> for SystemTime {
        type Output = Self;
        fn add(self, rhs: Duration) -> Self { self.checked_add(rhs).expect("SystemTime overflow") }
    }
    impl Sub<Duration> for SystemTime {
        type Output = Self;
        fn sub(self, rhs: Duration) -> Self { self.checked_sub(rhs).expect("SystemTime underflow") }
    }
    impl AddAssign<Duration> for SystemTime { fn add_assign(&mut self, rhs: Duration) { *self = *self + rhs; } }
    impl SubAssign<Duration> for SystemTime { fn sub_assign(&mut self, rhs: Duration) { *self = *self - rhs; } }
}
#[cfg(target_arch = "wasm32")]
pub use browser::*;
