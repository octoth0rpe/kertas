//! Images built into Kertas, for GPUI's `svg()` element.
//!
//! The alert icons are GitHub's Octicons (MIT; see the About tab).

use std::borrow::Cow;

use gpui::{AssetSource, Result, SharedString};
use kertas::rendered::AlertKind;

const FILES: &[(&str, &[u8])] = &[
    (
        "alerts/info.svg",
        include_bytes!("../assets/alerts/info.svg"),
    ),
    (
        "alerts/light-bulb.svg",
        include_bytes!("../assets/alerts/light-bulb.svg"),
    ),
    (
        "alerts/report.svg",
        include_bytes!("../assets/alerts/report.svg"),
    ),
    (
        "alerts/alert.svg",
        include_bytes!("../assets/alerts/alert.svg"),
    ),
    (
        "alerts/stop.svg",
        include_bytes!("../assets/alerts/stop.svg"),
    ),
];

pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        Ok(FILES
            .iter()
            .find(|(name, _)| *name == path)
            .map(|(_, bytes)| Cow::Borrowed(*bytes)))
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        Ok(FILES
            .iter()
            .filter(|(name, _)| name.starts_with(path))
            .map(|(name, _)| SharedString::from(*name))
            .collect())
    }
}

/// The icon GitHub shows in an alert's title.
pub fn alert_icon(kind: AlertKind) -> &'static str {
    match kind {
        AlertKind::Note => "alerts/info.svg",
        AlertKind::Tip => "alerts/light-bulb.svg",
        AlertKind::Important => "alerts/report.svg",
        AlertKind::Warning => "alerts/alert.svg",
        AlertKind::Caution => "alerts/stop.svg",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_alert_has_an_icon() {
        for kind in [
            AlertKind::Note,
            AlertKind::Tip,
            AlertKind::Important,
            AlertKind::Warning,
            AlertKind::Caution,
        ] {
            let svg = Assets.load(alert_icon(kind)).unwrap().unwrap();
            assert!(svg.starts_with(b"<svg"), "{kind:?}");
        }
    }
}
