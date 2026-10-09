//! Cached motion policy and interruptible, opacity-only settings transitions.

use crate::motion::{Easing, Frame, MotionClock, MotionPolicy, Tween};
use gpui::{App, Context, Global, Window};
use std::time::Duration;

#[derive(Default)]
struct SystemMotion {
    reduced: bool,
}

impl Global for SystemMotion {}

pub(super) fn init(cx: &mut App) {
    cx.set_global(SystemMotion {
        reduced: crate::platform::ui_motion::reduced_motion().unwrap_or(false),
    });
}

pub(super) fn observe_system_preferences<T: 'static>(window: &mut Window, cx: &mut Context<T>) {
    cx.observe_window_activation(window, |_, window, cx| {
        if window.is_window_active()
            && let Some(reduced) = crate::platform::ui_motion::reduced_motion()
            && let Some(state) = cx.try_global::<SystemMotion>()
            && state.reduced != reduced
        {
            cx.global_mut::<SystemMotion>().reduced = reduced;
            cx.refresh_windows();
        }
    })
    .detach();
}

pub(super) fn system_reduced_motion(cx: &App) -> bool {
    cx.reduce_motion() || cx.try_global::<SystemMotion>().is_some_and(|state| state.reduced)
}

pub(super) fn pointer_motion_enabled(window: &Window, cx: &App) -> bool {
    !window.last_input_was_keyboard() && super::config::animations_enabled(cx)
}

fn fade_policy(window: &Window, cx: &App) -> MotionPolicy {
    if window.last_input_was_keyboard()
        || cx.try_global::<super::config::Settings>().is_some_and(|settings| !settings.animations)
    {
        MotionPolicy::Off
    } else if system_reduced_motion(cx) {
        MotionPolicy::Reduced
    } else {
        MotionPolicy::Full
    }
}

/// Retarget from the displayed opacity when navigation interrupts a fade. No
/// retained page copy, input delay, background task, or independent timing math.
pub(super) struct ContentFade {
    section: Option<usize>,
    tween: Tween,
    clock: MotionClock,
    policy: MotionPolicy,
}

impl Default for ContentFade {
    fn default() -> Self {
        Self {
            section: None,
            tween: Tween::new(1.0),
            clock: MotionClock::default(),
            policy: MotionPolicy::Off,
        }
    }
}

impl ContentFade {
    fn sample(&mut self, section: usize, policy: MotionPolicy, frame: Frame) -> f32 {
        let changed = self.section.replace(section) != Some(section);
        if policy == MotionPolicy::Off {
            self.tween.snap_to(1.0);
        } else if changed || self.policy != policy {
            if changed && !self.tween.is_active() {
                self.tween.snap_to(0.0);
            }
            self.tween.animate_to(1.0, Duration::from_millis(180), Easing::UiEaseOut, policy);
        }
        self.policy = policy;
        self.tween.step(frame);
        self.tween.value()
    }

    pub(super) fn is_active(&self) -> bool {
        self.tween.is_active()
    }

    /// Apply this value to the caller's existing layout node. A new wrapper
    /// would change flex stretch/centering and the descendants' hit regions.
    pub(super) fn opacity(&mut self, section: usize, window: &mut Window, cx: &App) -> f32 {
        let frame = self.clock.tick();
        let opacity = self.sample(section, fade_policy(window, cx), frame);
        if self.is_active() {
            window.request_animation_frame();
        } else {
            self.clock.reset();
        }
        opacity
    }
}

/// CSS cubic-bezier(0.85, 0, 0.15, 1), including its time-axis inversion.
pub(super) fn ease(delta: f32) -> f32 {
    let delta = delta.clamp(0.0, 1.0);
    if delta == 0.0 || delta == 1.0 {
        return delta;
    }
    let mut low = 0.0;
    let mut high = 1.0;
    for _ in 0..16 {
        let t = (low + high) * 0.5;
        let u = 1.0 - t;
        let x = 3.0 * 0.85 * u * u * t + 3.0 * 0.15 * u * t * t + t * t * t;
        if x < delta {
            low = t;
        } else {
            high = t;
        }
    }
    let t = (low + high) * 0.5;
    t * t * (3.0 - 2.0 * t)
}

#[cfg(test)]
mod tests {
    use super::ease;

    #[test]
    fn content_fade_is_interruptible_and_reduced_motion_is_gentler() {
        use crate::motion::{Frame, MotionPolicy};
        use std::time::{Duration, Instant};
        let now = Instant::now();
        let mut fade = super::ContentFade::default();
        let frame = |millis| Frame { now, delta: Duration::from_millis(millis) };
        assert_eq!(fade.sample(0, MotionPolicy::Off, frame(0)), 1.0);
        assert_eq!(fade.sample(1, MotionPolicy::Full, frame(0)), 0.0);
        let current = fade.sample(1, MotionPolicy::Full, frame(30));
        assert!(current > 0.0 && current < 1.0);
        assert_eq!(fade.sample(2, MotionPolicy::Full, frame(0)), current);
        assert_eq!(fade.sample(2, MotionPolicy::Off, frame(0)), 1.0);
        assert!(!fade.is_active());
        assert_eq!(fade.sample(3, MotionPolicy::Reduced, frame(0)), 0.0);
        assert_eq!(fade.sample(3, MotionPolicy::Reduced, frame(120)), 1.0);
        assert!(!fade.is_active());
    }

    #[test]
    fn ui_ease_out_matches_the_exact_css_curve() {
        use crate::motion::Easing;
        assert_eq!(Easing::UiEaseOut.sample(0.0), 0.0);
        assert_eq!(Easing::UiEaseOut.sample(1.0), 1.0);
        // Parametric point t=0.5 on cubic-bezier(0.23, 1, 0.32, 1).
        assert!((Easing::UiEaseOut.sample(0.33125) - 0.875).abs() < 0.0001);
    }

    #[test]
    fn easing_matches_the_reference_time_axis_and_preserves_endpoints() {
        assert_eq!(ease(0.0), 0.0);
        assert_eq!(ease(1.0), 1.0);
        assert!((ease(0.3953125) - 0.15625).abs() < 0.0001);
        assert!((ease(0.6046875) - 0.84375).abs() < 0.0001);
        let samples = (0..=100).map(|step| ease(step as f32 / 100.0)).collect::<Vec<_>>();
        assert!(samples.windows(2).all(|pair| pair[0] <= pair[1]));
    }
}
