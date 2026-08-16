//! The air-quality alert toast.
//!
//! Drawn over whatever screen is showing, because the point of an alert is that
//! it reaches you while you are looking at something else.
//!
//! The widget comes from [`ratatui_toaster`]; the decision of *what* to say and
//! *when* it stops being said is `tui::app::alerts`. The engine here is built
//! fresh each frame from that state rather than kept alive between them: it is
//! a holder for "which toast, in which corner", and letting the app model own
//! the timing keeps rendering a pure function of state — which is what the
//! render tests rely on.

use ratatui::{Frame, layout::Rect, widgets::Widget};
use ratatui_toaster::{
    ToastBuilder, ToastEngine, ToastEngineBuilder, ToastMessage, ToastPosition, ToastType,
};

use crate::tui::app::{AirAlert, AlertKind, TuiApp};

/// Cells kept clear around the toast so it does not sit against the frame.
const TOAST_MARGIN: u16 = 1;

pub(super) fn render_alert(frame: &mut Frame<'_>, area: Rect, app: &TuiApp) {
    let Some(alert) = app.current_alert() else {
        return;
    };

    // Bottom-right: the top bar carries the live status and the bottom-left is
    // where the footer's key hints are, so this is the corner with least to
    // cover. It is also where the which-key popup appears, and only one of the
    // two is ever open.
    let toast_area = Rect {
        x: area.x,
        y: area.y,
        width: area.width.saturating_sub(TOAST_MARGIN),
        height: area.height.saturating_sub(TOAST_MARGIN),
    };

    let mut engine: ToastEngine<ToastMessage> = ToastEngineBuilder::new(toast_area).build();
    engine.show_toast(
        ToastBuilder::new(alert.message.clone().into())
            .toast_type(toast_type(alert))
            .position(ToastPosition::BottomRight),
    );

    (&engine).render(toast_area, frame.buffer_mut());
}

/// How loudly to say it. The toaster's own four levels map onto the reading's
/// band: the two worst are errors, anything else concerning is a warning, and
/// coming back down is good news rather than a problem.
fn toast_type(alert: &AirAlert) -> ToastType {
    use crate::sensors::Status;

    match alert.kind {
        AlertKind::Recovered => ToastType::Success,
        AlertKind::Worsened => match alert.status {
            Status::Unhealthy | Status::VeryUnhealthy => ToastType::Error,
            _ => ToastType::Warning,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sensors::Status;
    use std::time::UNIX_EPOCH;

    fn alert(kind: AlertKind, status: Status) -> AirAlert {
        AirAlert {
            metric_key: "pm25",
            kind,
            status,
            message: "PM2.5 is Unhealthy — 58.2 ug/m3".to_string(),
            raised_at: UNIX_EPOCH,
        }
    }

    #[test]
    fn the_worst_bands_are_errors_and_the_rest_are_warnings() {
        assert!(matches!(
            toast_type(&alert(AlertKind::Worsened, Status::Unhealthy)),
            ToastType::Error
        ));
        assert!(matches!(
            toast_type(&alert(AlertKind::Worsened, Status::VeryUnhealthy)),
            ToastType::Error
        ));
        assert!(matches!(
            toast_type(&alert(AlertKind::Worsened, Status::Elevated)),
            ToastType::Warning
        ));
    }

    #[test]
    fn recovery_is_good_news_whatever_band_it_came_from() {
        assert!(matches!(
            toast_type(&alert(AlertKind::Recovered, Status::Good)),
            ToastType::Success
        ));
    }
}
