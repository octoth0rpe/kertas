//! Fading the blocks that aren't being edited (#121).
//!
//! While a block is being edited, with `fade_other_blocks` on, every other
//! item is drawn at [`FADED_OPACITY`]: text, images, rules and alert colors
//! alike, since the opacity applies to the whole item. When the edited
//! block changes, or editing starts or stops, each item's opacity moves to
//! its new one over [`TRANSITION`].

use std::time::{Duration, Instant};

/// How opaque a faded item is. Faded body text keeps at least 3:1 contrast
/// against the background in every built-in theme (see the theme tests).
pub(crate) const FADED_OPACITY: f32 = 0.75;

/// How long the fade takes to follow the edited block.
const TRANSITION: Duration = Duration::from_millis(150);

/// Which items are drawn unfaded, and which were before the last change.
#[derive(Debug, Clone, Default)]
pub(super) struct Fade {
    /// The items being edited, which aren't faded, or `None` if nothing is
    /// faded.
    edited: Option<Vec<usize>>,
    /// What `edited` was before it last changed.
    previous: Option<Vec<usize>>,
    /// When it last changed, or `None` if it never has.
    changed_at: Option<Instant>,
}

impl Fade {
    /// Notes that the items being edited are now `edited` (`None` if
    /// nothing is to be faded), starting a transition if that's a change.
    pub(super) fn update(&mut self, edited: Option<Vec<usize>>, now: Instant) {
        if edited != self.edited {
            self.previous = std::mem::replace(&mut self.edited, edited);
            self.changed_at = Some(now);
        }
    }

    /// How far through the transition it is at `now`, from 0 to 1.
    fn progress(&self, now: Instant) -> f32 {
        match self.changed_at {
            Some(changed_at) => {
                (now.duration_since(changed_at).as_secs_f32() / TRANSITION.as_secs_f32()).min(1.)
            }
            None => 1.,
        }
    }

    /// Whether a transition is still under way at `now`, so the view needs
    /// drawing again next frame.
    pub(super) fn animating(&self, now: Instant) -> bool {
        self.progress(now) < 1.
    }

    /// Item `index`'s opacity at `now`.
    pub(super) fn opacity(&self, index: usize, now: Instant) -> f32 {
        let target = opacity_in(self.edited.as_deref(), index);
        let progress = self.progress(now);
        if progress >= 1. {
            return target;
        }
        let start = opacity_in(self.previous.as_deref(), index);
        // Eased out, so the change starts quickly and settles gently.
        let eased = 1. - (1. - progress).powi(2);
        start + (target - start) * eased
    }
}

/// Item `index`'s opacity while `edited` are the items being edited.
fn opacity_in(edited: Option<&[usize]>, index: usize) -> f32 {
    match edited {
        Some(edited) if !edited.contains(&index) => FADED_OPACITY,
        _ => 1.,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_is_faded_until_a_block_is_edited() {
        let fade = Fade::default();
        assert_eq!(fade.opacity(3, Instant::now()), 1.);
    }

    #[test]
    fn every_block_but_the_edited_one_fades() {
        let start = Instant::now();
        let mut fade = Fade::default();
        fade.update(Some(vec![2]), start);
        let later = start + TRANSITION;
        assert_eq!(fade.opacity(2, later), 1.);
        assert_eq!(fade.opacity(1, later), FADED_OPACITY);
        assert_eq!(fade.opacity(3, later), FADED_OPACITY);
        assert!(!fade.animating(later));
    }

    #[test]
    fn moving_to_another_block_fades_between_them() {
        let start = Instant::now();
        let mut fade = Fade::default();
        fade.update(Some(vec![2]), start - TRANSITION);
        fade.update(Some(vec![3]), start);
        let halfway = start + TRANSITION / 2;
        assert!(fade.animating(halfway));
        let leaving = fade.opacity(2, halfway);
        let arriving = fade.opacity(3, halfway);
        assert!(leaving < 1. && leaving > FADED_OPACITY);
        assert!(arriving < 1. && arriving > FADED_OPACITY);
        // Others stay faded throughout.
        assert_eq!(fade.opacity(5, halfway), FADED_OPACITY);
    }

    #[test]
    fn stopping_editing_unfades_everything() {
        let start = Instant::now();
        let mut fade = Fade::default();
        fade.update(Some(vec![2]), start - TRANSITION);
        fade.update(None, start);
        assert_eq!(fade.opacity(5, start + TRANSITION), 1.);
    }

    #[test]
    fn the_same_block_starts_no_transition() {
        let start = Instant::now();
        let mut fade = Fade::default();
        fade.update(Some(vec![2]), start - TRANSITION);
        fade.update(Some(vec![2]), start);
        assert!(!fade.animating(start));
    }
}
