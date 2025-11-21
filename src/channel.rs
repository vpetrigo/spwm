//! Channel management and builder pattern implementation for SPWM.
//!
//! This module provides the `SpwmChannel` struct and a type-safe builder pattern
//! for creating and configuring individual PWM channels.

use crate::{OnOffCallback, PeriodCallback, SpwmError, SpwmState};
use core::cell::OnceCell;
use core::marker::PhantomData;
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

/// Maximum allowed duty cycle percentage.
const MAX_DUTY_CYCLE: u8 = 100;

/// Minimum ratio between hardware timer frequency and channel frequency.
/// The hardware timer must run at least 100x faster than the PWM channel frequency.
const FREQUENCY_DIFFERENCE_REQUIRED: u32 = 100;

/// Builder state indicating frequency needs to be set.
pub struct SpwmChannelFreqHzBuildState {}

/// Builder state indicating duty cycle needs to be set.
pub struct SpwmChannelDutyCycleBuildState {}

/// Builder state indicating channel is ready to build.
pub struct SpwmChannelFinalizedBuildState {}

/// Represents a single PWM channel with its configuration and state.
///
/// Each channel maintains its own timing counters, callbacks, and enable state.
/// All fields use atomic operations for thread-safe access from interrupt contexts.
#[derive(Default, Debug)]
pub struct SpwmChannel {
    /// Total ticks in one PWM period
    pub(crate) period_ticks: AtomicU32,
    /// Number of ticks the output stays "on" in the current period
    pub(crate) on_ticks: AtomicU32,
    /// Pending `on_ticks` value to be applied at next period start
    pub(crate) update_on_ticks: AtomicU32,
    /// Current tick counter within the period
    pub(crate) counter: AtomicU32,
    /// Whether this channel is currently enabled
    pub(crate) enabled: AtomicBool,
    /// Callback invoked on state changes
    pub(crate) on_off_callback: OnceCell<OnOffCallback>,
    /// Callback invoked at period completion
    pub(crate) period_callback: OnceCell<PeriodCallback>,
}

impl SpwmChannel {
    /// Increments and returns the current tick counter.
    pub(crate) fn counter_tick(&self) -> u32 {
        self.counter.fetch_add(1, Ordering::SeqCst)
    }

    /// Resets the tick counter to zero (called at period boundaries).
    pub(crate) fn counter_reset(&self) {
        self.counter.store(0, Ordering::SeqCst);
    }

    /// Sets the total number of ticks in one PWM period.
    pub(crate) fn set_period_ticks(&self, period_ticks: u32) {
        self.period_ticks.store(period_ticks, Ordering::SeqCst);
    }

    /// Updates the on-time ticks, applying immediately if disabled or at next period if enabled.
    pub(crate) fn update_on_ticks(&self, on_ticks: u32) {
        if self.enabled.load(Ordering::Relaxed) {
            self.update_on_ticks.store(on_ticks, Ordering::SeqCst);
        } else {
            self.on_ticks.store(on_ticks, Ordering::SeqCst);
            self.update_on_ticks.store(on_ticks, Ordering::SeqCst);
        }
    }

    /// Sets the on-time ticks directly (used internally by IRQ handler).
    pub(crate) fn set_on_ticks(&self, on_ticks: u32) {
        self.on_ticks.store(on_ticks, Ordering::SeqCst);
    }

    /// Sets the on/off state change callback. Can only be called once.
    pub(crate) fn set_on_off_callback(
        &self,
        on_off_callback: OnOffCallback,
    ) -> Result<(), OnOffCallback> {
        self.on_off_callback.set(on_off_callback)
    }

    /// Sets the period completion callback. Can only be called once.
    pub(crate) fn set_period_callback(
        &self,
        period_callback: PeriodCallback,
    ) -> Result<(), PeriodCallback> {
        self.period_callback.set(period_callback)
    }

    /// Updates the PWM frequency for this channel.
    ///
    /// # Parameters
    /// - `freq_hz`: Desired PWM frequency in Hz
    /// - `hardware_freq_hz`: Hardware timer frequency in Hz
    ///
    /// # Errors
    /// Returns `SpwmError::InvalidFrequency` if the frequency is 0 or too high relative
    /// to the hardware timer frequency (must be at least 100x lower).
    pub fn update_frequency(&self, freq_hz: u32, hardware_freq_hz: u32) -> Result<(), SpwmError> {
        input_frequency_validate(freq_hz, hardware_freq_hz)?;
        let ticks = hardware_freq_hz / freq_hz;
        self.set_period_ticks(ticks);

        Ok(())
    }

    /// Updates the duty cycle for this channel.
    ///
    /// # Parameters
    /// - `duty_cycle`: Duty cycle percentage (0-100)
    ///
    /// # Errors
    /// Returns `SpwmError::InvalidDutyCycle` if the duty cycle is greater than 100.
    pub fn update_duty_cycle(&self, duty_cycle: u8) -> Result<(), SpwmError> {
        if duty_cycle > MAX_DUTY_CYCLE {
            return Err(SpwmError::InvalidDutyCycle);
        }

        let period_ticks = self.period_ticks.load(Ordering::Relaxed);
        self.update_on_ticks(period_ticks / 100 * u32::from(duty_cycle));

        Ok(())
    }

    /// Enables the channel and invokes the on/off callback with the initial state.
    ///
    /// # Errors
    /// Returns `SpwmError::AlreadyEnabled` if the channel is already enabled, or
    /// `SpwmError::EnableFailed` if the atomic compare-exchange operation fails.
    pub fn enable(&self) -> Result<(), SpwmError> {
        let expected = false;

        if let Err(value) =
            self.enabled
                .compare_exchange(expected, true, Ordering::SeqCst, Ordering::SeqCst)
        {
            if value {
                return Err(SpwmError::AlreadyEnabled);
            }

            return Err(SpwmError::EnableFailed);
        }

        if let Some(callback) = self.on_off_callback.get()
            && self.on_ticks.load(Ordering::Relaxed) != 0
        {
            callback(&SpwmState::On);
        }

        Ok(())
    }

    /// Disables the channel, resets the counter, and invokes the on/off callback with Off state.
    ///
    /// # Errors
    /// Returns `SpwmError::AlreadyDisabled` if the channel is already disabled, or
    /// `SpwmError::DisableFailed` if the atomic compare-exchange operation fails.
    pub fn disable(&self) -> Result<(), SpwmError> {
        let expected = true;

        if let Err(value) =
            self.enabled
                .compare_exchange(expected, false, Ordering::SeqCst, Ordering::SeqCst)
        {
            if !value {
                return Err(SpwmError::AlreadyDisabled);
            }

            return Err(SpwmError::DisableFailed);
        }

        self.counter.store(0, Ordering::Relaxed);

        if let Some(callback) = self.on_off_callback.get() {
            callback(&SpwmState::Off);
        }

        Ok(())
    }
}

/// Type-safe builder for creating PWM channels.
///
/// The builder uses phantom types to enforce the correct correct configuration order:
/// 1. Optionally set callbacks
/// 2. Set frequency (required)
/// 3. Set duty cycle (required)
/// 4. Build the channel
///
/// # Type Parameter
/// - `T`: Current build state (`FreqHz`, `DutyCycle`, or `Finalized`)
pub struct SpwmChannelBuilder<T> {
    hardware_freq_hz: u32,
    channel_freq_hz: u32,
    duty_cycle: u8,
    on_off_callback: Option<OnOffCallback>,
    period_callback: Option<PeriodCallback>,
    _phantom: PhantomData<T>,
}

impl<T> SpwmChannelBuilder<T> {
    #[must_use]
    pub fn on_off_callback(mut self, on_off_callback: OnOffCallback) -> Self {
        self.on_off_callback = Some(on_off_callback);
        self
    }

    #[must_use]
    pub fn period_callback(mut self, period_callback: PeriodCallback) -> Self {
        self.period_callback = Some(period_callback);
        self
    }
}

impl SpwmChannelBuilder<SpwmChannelFreqHzBuildState> {
    /// Creates a new channel builder (called internally by `Spwm::create_channel()`).
    #[must_use]
    pub fn new(hardware_freq_hz: u32) -> Self {
        Self {
            hardware_freq_hz,
            channel_freq_hz: 0,
            duty_cycle: 0,
            on_off_callback: None,
            period_callback: None,
            _phantom: PhantomData,
        }
    }

    #[must_use]
    pub fn freq_hz(self, freq_hz: u32) -> SpwmChannelBuilder<SpwmChannelDutyCycleBuildState> {
        SpwmChannelBuilder {
            hardware_freq_hz: self.hardware_freq_hz,
            channel_freq_hz: freq_hz,
            duty_cycle: 0,
            on_off_callback: self.on_off_callback,
            period_callback: self.period_callback,
            _phantom: PhantomData,
        }
    }
}

impl SpwmChannelBuilder<SpwmChannelDutyCycleBuildState> {
    #[must_use]
    pub fn duty_cycle(self, duty_cycle: u8) -> SpwmChannelBuilder<SpwmChannelFinalizedBuildState> {
        SpwmChannelBuilder {
            hardware_freq_hz: self.hardware_freq_hz,
            channel_freq_hz: self.channel_freq_hz,
            duty_cycle,
            on_off_callback: self.on_off_callback,
            period_callback: self.period_callback,
            _phantom: PhantomData,
        }
    }
}

impl SpwmChannelBuilder<SpwmChannelFinalizedBuildState> {
    /// Builds and validates the PWM channel.
    ///
    /// # Errors
    /// Returns an error if:
    /// - `SpwmError::InvalidHardwareFrequency` if the hardware frequency is 0
    /// - `SpwmError::InvalidFrequency` if the channel frequency is invalid
    /// - `SpwmError::InvalidDutyCycle` if the duty cycle is greater than 100
    /// - `SpwmError::CallbackSetError` if callbacks are not set or failed to be set
    pub fn build(self) -> Result<SpwmChannel, SpwmError> {
        if self.hardware_freq_hz == 0 {
            return Err(SpwmError::InvalidHardwareFrequency);
        }

        let channel = SpwmChannel::default();

        channel.update_frequency(self.channel_freq_hz, self.hardware_freq_hz)?;
        channel.update_duty_cycle(self.duty_cycle)?;

        match self.on_off_callback {
            Some(cb) => channel
                .set_on_off_callback(cb)
                .map_err(|_| SpwmError::CallbackSetError)?,
            None => {
                return Err(SpwmError::CallbackSetError);
            }
        }

        match self.period_callback {
            Some(cb) => channel
                .set_period_callback(cb)
                .map_err(|_| SpwmError::CallbackSetError)?,
            None => {
                return Err(SpwmError::CallbackSetError);
            }
        }

        Ok(channel)
    }
}

fn input_frequency_validate(freq_hz: u32, hardware_freq_hz: u32) -> Result<(), SpwmError> {
    if freq_hz == 0 || freq_hz > hardware_freq_hz / FREQUENCY_DIFFERENCE_REQUIRED {
        return Err(SpwmError::InvalidFrequency);
    }

    Ok(())
}
