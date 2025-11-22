//! # SPWM - Software PWM for Embedded Systems
//!
//! ------------------------------------------
//!
//! A `no_std` Rust library for generating software-based Pulse Width Modulation (PWM) signals on microcontrollers and
//! embedded systems. This crate provides a flexible, interrupt-driven PWM implementation that doesn't require dedicated
//! hardware PWM peripherals.
//!
//! ## Features
//!
//! - **`no_std` compatible** - Works in embedded environments without the standard library
//! - **Multiple independent channels** - Configure up to N channels (compile-time constant)
//! - **Thread-safe** - Uses atomic operations for safe access from interrupt contexts
//! - **Type-safe builder pattern** - Compile-time guarantees for proper channel configuration
//! - **Flexible callbacks** - Register callbacks for state changes and period completion
//! - **Dynamic updates** - Change frequency and duty cycle at runtime
//!
//! ## Basic Usage
//!
//! Add this to your `Cargo.toml`:
//!
//! ```toml
//! [dependencies]
//! spwm = "0.1"
//! ```
//!
//! ### Creating a Simple PWM Channel
//!
//! ```rust
//! use spwm::{Spwm, SpwmState};
//! // Create SPWM manager with hardware timer frequency of 100 kHz
//! // and space for 4 channels
//! let mut spwm = Spwm::<4>::new(100_000);
//! // Create a channel with 1 kHz frequency and 50% duty cycle
//! let channel = spwm
//!     .create_channel()
//!     .freq_hz(1_000)
//!     .duty_cycle(50)
//!     .on_off_callback(|state: &SpwmState| {
//!         match state {
//!             SpwmState::On => {
//!                 // Turn your output pin HIGH
//!             }
//!             SpwmState::Off => {
//!                 // Turn your output pin LOW
//!             }
//!         }
//!     })
//!     .period_callback(|| {
//!         // Called at the end of each PWM period
//!     })
//!     .build()?;
//! let channel_id = spwm.register_channel(channel)?;
//!
//! // Enable the channel to start PWM generation
//! spwm.get_channel(channel_id).unwrap().enable()?;
//! ```
//!
//! ### In Your Timer Interrupt Handler
//!
//! ```rust
//! #[interrupt]
//! fn TIMER_IRQ() {
//!     spwm.irq_handler();
//! }
//! ```
//!
//! ## Requirements
//!
//! - Hardware timer that can interrupt at a consistent frequency
//! - Timer frequency must be at least 100x the desired PWM channel frequency to achieve 1% duty cycle resolution
//!   capabilities
//! - Callbacks must be short and non-blocking (they run in interrupt context)
//!
//! ## Example
//!
//! - [STM32 Nucleo-F302R8 board example](https://github.com/vp-supplementary/nucleo-f302-spwm): 4-channel software PWM
//!   output
//!
//! ```rust
//! use spwm::{Spwm, SpwmState};
//!
//! static mut LED_STATE: bool = false;
//!
//! fn led_callback(state: &SpwmState) {
//!     unsafe {
//!         LED_STATE = matches!(state, SpwmState::On);
//!         // Update your LED pin based on LED_STATE
//!     }
//! }
//!
//! let mut pwm = Spwm::<1>::new(100_000);
//! let channel = pwm
//!     .create_channel()
//!     .freq_hz(100) // 100 Hz PWM frequency
//!     .duty_cycle(25) // 25% brightness
//!     .on_off_callback(led_callback)
//!     .period_callback(|| {})
//!     .build()?;
//!
//! let id = pwm.register_channel(channel)?;
//! pwm.get_channel(id).unwrap().enable()?;
//! ```
#![no_std]
mod channel;

use core::sync::atomic::Ordering;

pub use channel::{SpwmChannel, SpwmChannelBuilder, SpwmChannelFreqHzBuildState};

/// Represents the output state of a PWM channel.
pub enum SpwmState {
    /// Output is in the "on" (high) state
    On,
    /// Output is in the "off" (low) state
    Off,
}

/// Errors that can occur during SPWM operations.
#[derive(Debug, PartialEq)]
pub enum SpwmError {
    /// The specified hardware timer frequency is not valid
    InvalidHardwareFrequency,
    /// The specified channel index is out of range
    InvalidChannel,
    /// The requested frequency is too high for the configured hardware timer frequency
    InvalidFrequency,
    /// The duty cycle value is greater than 100
    InvalidDutyCycle,
    /// Failed to set a callback (already set or required callback missing)
    CallbackSetError,
    /// A PWM channel already enabled
    AlreadyEnabled,
    /// A PWM channel enable operation failed
    EnableFailed,
    /// A PWM channel already disabled
    AlreadyDisabled,
    /// A PWM channel disable operation failed
    DisableFailed,
    /// No free channel slots available for registration
    NoChannelSlotAvailable,
}

/// Callback invoked when a channel's output state changes.
///
/// # Parameters
/// - `state`: The new state of the channel output
pub type OnOffCallback = fn(&SpwmState);

/// Callback invoked at the end of each PWM period.
pub type PeriodCallback = fn();

/// Callback invoked when the first channel is enabled (timer should start).
pub type TimerStartCallback = fn();

/// Callback invoked when all channels are disabled (timer can stop).
pub type TimerStopCallback = fn();

/// Unique identifier for a registered channel.
pub type ChannelId = usize;

/// A container structure used to hold an optional `SpwmChannel`.
///
/// # Fields
///
/// - `channel`:
///   An `Option<SpwmChannel>` instance, which can contain either:
///   - `Some(SpwmChannel)`: A valid `SpwmChannel` object.
///   - `None`: Indicates the absence of a channel.
#[derive(Default)]
struct ChannelSlot {
    channel: Option<SpwmChannel>,
}

/// A structure for managing Software Pulse Width Modulation (SPWM) channels.
///
/// This struct defines a configurable software-based PWM system with a fixed
/// number of channels. Each channel can be individually controlled via its
/// corresponding `ChannelSlot`.
///
/// # Type Parameters
///
/// - `N`: The number of PWM channels, which determines the size of the `channel_slots` array.
///
/// # Fields
/// - `channel_slots`: An array of `ChannelSlot` instances representing individual
///   PWM channels. Each channel can be configured and utilized independently.
/// - `freq_hz`: The frequency of the PWM signal in hertz (Hz).
///
/// # Example
///
/// ```
/// # use spwm::Spwm;
/// // Example usage with 4 PWM channels
/// let spwm: Spwm<4> = Spwm::new(100_000);
/// ```
///
/// # Notes
///
/// - The array size for `channel_slots` is determined at compile-time via the generic
///   `N` parameter, ensuring that the implementation is efficient and tailored to the
///   user's requirements.
pub struct Spwm<const N: usize> {
    channel_slots: [ChannelSlot; N],
    freq_hz: u32,
}

impl<const N: usize> Spwm<N> {
    /// Creates a new instance with the specified frequency (in Hertz).
    ///
    /// # Parameters
    ///
    /// - `freq_hz`: The frequency in Hertz to initialize the instance with.
    ///
    /// # Returns
    ///
    /// A new instance of the struct is initialized with `freq_hz` and default values
    /// for `channel_slots`.
    ///
    /// # Attributes
    ///
    /// - `#[must_use]`: Indicates that the returned instance must be used;
    ///   ignoring it may lead to unexpected behavior or logic bugs.
    #[must_use]
    pub fn new(freq_hz: u32) -> Self {
        Self {
            freq_hz,
            channel_slots: core::array::from_fn(|_| ChannelSlot::default()),
        }
    }

    /// Creates a new SPWM (Sinusoidal Pulse Width Modulation) channel builder.
    ///
    /// This function initializes and returns an `SpwmChannelBuilder` in the
    /// `SpwmChannelFreqHzBuildState`, which uses the frequency (in Hz) specified
    /// by the `freq_hz` field of the current instance. The returned builder can
    /// then be used to configure and build an SPWM channel.
    ///
    /// # Returns
    ///
    /// An instance of `SpwmChannelBuilder<SpwmChannelFreqHzBuildState>` configured
    /// with the frequency from the current instance.
    ///
    /// # Example
    ///
    /// ```
    /// # use spwm::Spwm;
    /// let spwm: Spwm<1> = Spwm::new(1_000_000); // Example initialization
    /// let channel_builder = spwm.create_channel();
    /// // Further configuration can be done using the returned builder
    /// ```
    pub fn create_channel(&self) -> SpwmChannelBuilder<SpwmChannelFreqHzBuildState> {
        SpwmChannelBuilder::new(self.freq_hz)
    }

    /// Registers a PWM channel and returns its unique identifier.
    ///
    /// # Parameters
    /// - `channel`: The PWM channel to register
    ///
    /// # Errors
    /// Returns `SpwmError::NoChannelSlotAvailable` if all channel slots are already occupied.
    pub fn register_channel(&mut self, channel: SpwmChannel) -> Result<ChannelId, SpwmError> {
        for (i, slot) in self.channel_slots.iter_mut().enumerate() {
            if slot.channel.is_none() {
                slot.channel = Some(channel);

                return Ok(i);
            }
        }

        Err(SpwmError::NoChannelSlotAvailable)
    }

    /// Retrieves a reference to a `SpwmChannel` associated with the specified `channel_id`,
    /// if it exists.
    ///
    /// # Parameters
    ///
    /// - `channel_id`: The identifier of the channel being requested.
    ///
    /// # Returns
    ///
    /// PWM channel reference if it exists, or `None` otherwise.
    ///
    /// # Examples
    /// ```
    /// # use spwm::Spwm;
    /// let mut spwm: Spwm<1> = Spwm::new(1_000_000); // Example initialization
    /// let led_control_channel = spwm.create_channel()
    ///     .freq_hz(100)
    ///     .duty_cycle(5)
    ///     .on_off_callback(|_| {})
    ///     .period_callback(|| {}).build();
    /// let channel = led_control_channel.unwrap();
    /// let led_control_channel_id = spwm.register_channel(channel).unwrap();
    /// // Other code here ...
    /// let led_control_channel = spwm.get_channel(led_control_channel_id).unwrap();
    /// ```
    pub fn get_channel(&self, channel_id: ChannelId) -> Option<&SpwmChannel> {
        self.channel_slots.get(channel_id)?.channel.as_ref()
    }

    /// Handles the Interrupt Request (IRQ) for Pulse Width Modulation (PWM) channels.
    ///
    /// This function is invoked to process the state of all PWM channel slots when an IRQ occurs.
    /// It ensures that the PWM signals operate, according to their defined periods, on-times, and
    /// triggers appropriate callbacks when specific events occur.
    ///
    /// # Example
    /// ```
    /// #[interrupt]
    /// fn TIMER_IRQ() {
    ///     spwm.irq_handler();
    /// }
    /// ```
    pub fn irq_handler(&self) {
        for slot in &self.channel_slots {
            if let Some(ref channel) = slot.channel
                && channel.enabled.load(Ordering::Relaxed)
            {
                let current_ticks = channel.counter_tick();
                let period_ticks = channel.period_ticks.load(Ordering::Relaxed);
                let on_ticks = channel.on_ticks.load(Ordering::Relaxed);

                if current_ticks >= (period_ticks - 1) {
                    let update_ticks = channel.update_on_ticks.load(Ordering::Relaxed);

                    channel.counter_reset();

                    if let Some(callback) = channel.period_callback.get() {
                        callback();
                    }

                    if update_ticks != on_ticks {
                        channel.set_on_ticks(update_ticks);
                    }

                    let on_ticks = channel.on_ticks.load(Ordering::Relaxed);

                    if on_ticks != 0
                        && let Some(callback) = channel.on_off_callback.get()
                    {
                        callback(&SpwmState::On);
                    }
                } else if current_ticks == on_ticks
                    && let Some(callback) = channel.on_off_callback.get()
                {
                    callback(&SpwmState::Off);
                }
            }
        }
    }
}
