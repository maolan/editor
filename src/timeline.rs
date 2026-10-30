use maolan_widgets::iced::{
    Color, Element, Length, Point, Rectangle, Renderer, Theme, mouse,
    widget::{
        canvas,
        canvas::{Frame, Geometry, Path, Program, Stroke},
    },
};

use crate::{message::Message, state::EditApp};

const TIMELINE_HEIGHT: f32 = 26.0;
const TARGET_LABEL_SPACING: f32 = 100.0;

struct Timeline {
    start_seconds: f64,
    end_seconds: f64,
}

pub(crate) fn timeline(app: &EditApp) -> Element<'static, Message> {
    let (start_seconds, end_seconds) = app.audio.as_ref().map_or((0.0, 0.0), |audio| {
        let duration = audio.frames() as f64 / audio.sample_rate.max(1) as f64;
        let (start, end) = app.waveform_view.unwrap_or((0.0, 1.0));
        (duration * start as f64, duration * end as f64)
    });
    canvas(Timeline {
        start_seconds,
        end_seconds,
    })
    .width(Length::Fill)
    .height(Length::Fixed(TIMELINE_HEIGHT))
    .into()
}

impl<Message> Program<Message> for Timeline {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        frame.fill(
            &Path::rectangle(Point::ORIGIN, bounds.size()),
            Color::from_rgb(0.075, 0.078, 0.095),
        );
        frame.stroke(
            &Path::line(
                Point::new(0.0, bounds.height - 0.5),
                Point::new(bounds.width, bounds.height - 0.5),
            ),
            Stroke::default()
                .with_color(Color::from_rgb(0.18, 0.20, 0.24))
                .with_width(1.0),
        );
        let span = self.end_seconds - self.start_seconds;
        if span <= 0.0 || bounds.width <= 0.0 {
            return vec![frame.into_geometry()];
        }

        let step = nice_step(span / (bounds.width / TARGET_LABEL_SPACING).max(1.0) as f64);
        let first = (self.start_seconds / step).ceil() * step;
        for tick_index in 0..1000 {
            let seconds = first + tick_index as f64 * step;
            if seconds > self.end_seconds + step * 1e-6 {
                break;
            }
            let x = ((seconds - self.start_seconds) / span * bounds.width as f64) as f32;
            draw_tick(&mut frame, bounds, x, true);
            frame.fill_text(canvas::Text {
                content: format_time(seconds, step),
                position: Point::new(x + 4.0, 2.0),
                color: Color::from_rgb(0.78, 0.81, 0.86),
                size: 10.0.into(),
                ..Default::default()
            });
            for minor in 1..5 {
                let minor_seconds = seconds + step * minor as f64 / 5.0;
                if minor_seconds >= self.end_seconds {
                    break;
                }
                let minor_x =
                    ((minor_seconds - self.start_seconds) / span * bounds.width as f64) as f32;
                draw_tick(&mut frame, bounds, minor_x, false);
            }
        }

        vec![frame.into_geometry()]
    }
}

fn draw_tick(frame: &mut Frame, bounds: Rectangle, x: f32, major: bool) {
    let height = if major { 9.0 } else { 4.0 };
    frame.stroke(
        &Path::line(
            Point::new(x, bounds.height - height),
            Point::new(x, bounds.height),
        ),
        Stroke::default()
            .with_color(if major {
                Color::from_rgb(0.55, 0.59, 0.66)
            } else {
                Color::from_rgb(0.30, 0.33, 0.39)
            })
            .with_width(1.0),
    );
}

fn nice_step(rough_step: f64) -> f64 {
    if rough_step <= 0.0 {
        return 1.0;
    }
    let exponent = rough_step.log10().floor() as i32;
    let scale = 10_f64.powi(exponent);
    [1.0, 2.0, 5.0, 10.0]
        .into_iter()
        .map(|multiplier| multiplier * scale)
        .min_by(|a, b| (a - rough_step).abs().total_cmp(&(b - rough_step).abs()))
        .unwrap_or(1.0)
}

fn format_time(seconds: f64, step: f64) -> String {
    let decimals = if step >= 1.0 {
        0
    } else {
        (-step.log10()).ceil().clamp(0.0, 6.0) as usize
    };
    let factor = 10_f64.powi(decimals as i32);
    let seconds = (seconds * factor).round() / factor;
    let minutes = (seconds / 60.0).floor() as u64;
    let seconds = seconds - minutes as f64 * 60.0;
    if decimals == 0 {
        format!("{minutes:02}:{seconds:02.0}")
    } else {
        let width = decimals + 3;
        let precision = decimals;
        format!("{minutes:02}:{seconds:0width$.precision$}")
    }
}

#[cfg(test)]
mod tests {
    use super::{format_time, nice_step};

    #[test]
    fn nice_step_selects_readable_intervals() {
        assert_eq!(nice_step(0.8), 1.0);
        assert_eq!(nice_step(3.0), 2.0);
        assert_eq!(nice_step(4.0), 5.0);
        assert_eq!(nice_step(80.0), 100.0);
    }

    #[test]
    fn format_time_adapts_precision_to_tick_spacing() {
        assert_eq!(format_time(0.0, 1.0), "00:00");
        assert_eq!(format_time(61.5, 0.1), "01:01.5");
        assert_eq!(format_time(3600.0, 10.0), "60:00");
        assert_eq!(format_time(0.001, 0.001), "00:00.001");
    }

    #[test]
    fn format_time_rounds_before_splitting_minutes() {
        assert_eq!(format_time(59.999999, 0.1), "01:00.0");
    }
}
