//! Live TorrServer throughput gauge.

use std::sync::{Arc, Mutex, MutexGuard};

use cinebox_core::i18n::Msg;
use cinebox_torrserver::SpeedEvent;
use egui::{Align2, Pos2, RichText, Sense, Shape, Stroke, Ui, pos2, vec2};

use crate::jobs::{self, JobError, TorrCtx};
use crate::theme::Theme;

const MIN_SCALE: f64 = 20.0;
const HEADROOM: f64 = 1.25;
const SMOOTH_TAU: f64 = 1.2;
const SWING_SECS: f64 = 3.5;
const SPAN_SECS: f64 = 1.6;
const SETTLE_FRAC: f64 = 0.003;
const NEEDLE_RIM: f64 = 0.82;

#[derive(Clone)]
pub struct SpeedMeter {
    inner: Arc<Mutex<Live>>,
}

#[derive(Clone)]
struct Live {
    phase: Phase,
    target: f64,
    shown: f64,
    span: f64,
    last_t: Option<f64>,
    error: Option<String>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Idle,
    Connecting,
    Testing,
    Ready,
    Failed,
}

impl Default for Live {
    fn default() -> Self {
        Self {
            phase: Phase::Idle,
            target: 0.0,
            shown: 0.0,
            span: MIN_SCALE,
            last_t: None,
            error: None,
        }
    }
}

impl SpeedMeter {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(Live::default())),
        }
    }

    fn lock(&self) -> MutexGuard<'_, Live> {
        self.inner.lock().unwrap_or_else(|poison| poison.into_inner())
    }

    pub fn is_busy(&self) -> bool {
        matches!(self.lock().phase, Phase::Connecting | Phase::Testing)
    }

    pub fn needs_repaint(&self) -> bool {
        let live = self.lock();
        if matches!(live.phase, Phase::Connecting | Phase::Testing) {
            return true;
        }

        let span = live.span.max(MIN_SCALE);
        let needed = needed_span(&live);
        let catching_up = (live.shown - live.target).abs() > settle_eps(span);
        let scaling = needed > span + settle_eps(needed);
        catching_up || scaling
    }

    pub fn begin(&self) {
        let mut live = self.lock();
        live.phase = Phase::Connecting;
        live.target = 0.0;
        live.shown = 0.0;
        live.span = MIN_SCALE;
        live.last_t = None;
        live.error = None;
    }

    pub fn on_event(&self, event: SpeedEvent) {
        let mut live = self.lock();
        match event {
            SpeedEvent::Testing => {
                live.phase = Phase::Testing;
            }
            SpeedEvent::Sample { mbps, .. } => {
                live.phase = Phase::Testing;
                live.target = mbps;
            }
        }
    }

    pub fn finish_ok(&self, mbps: f64) {
        let mut live = self.lock();
        live.target = mbps;
        live.phase = Phase::Ready;
        live.error = None;
    }

    pub fn finish_err(&self, msg: String) {
        let mut live = self.lock();
        live.phase = Phase::Failed;
        live.error = Some(msg);
    }

    fn tick(&self, now: f64) {
        let mut live = self.lock();
        let dt = match live.last_t {
            Some(prev) => (now - prev).clamp(0.0, 0.05),
            None => 1.0 / 60.0,
        };
        live.last_t = Some(now);

        grow_span(&mut live, dt);
        ease_shown(&mut live, dt);
    }
}

pub async fn run(torr: TorrCtx, meter: SpeedMeter, ctx: egui::Context) -> Result<(), JobError> {
    let live = meter.clone();
    let live_ctx = ctx.clone();
    let result = jobs::speed_test(torr, move |event| {
        live.on_event(event);
        live_ctx.request_repaint();
    })
    .await;

    match &result {
        Ok(mbps) => meter.finish_ok(*mbps),
        Err(error) => meter.finish_err(error.to_string()),
    }

    ctx.request_repaint();
    result.map(|_| ())
}

pub fn paint(ui: &mut Ui, theme: &Theme, meter: &SpeedMeter) {
    let now = ui.input(|i| i.time);
    meter.tick(now);
    let live = meter.lock().clone();
    paint_gauge(ui, theme, &live);

    let Some(error) = live.error.as_deref() else {
        return;
    };

    ui.add_space(4.0);
    ui.label(RichText::new(error).size(theme.text_small).color(theme.err));
}

fn paint_gauge(ui: &mut Ui, theme: &Theme, live: &Live) {
    let width = ui.available_width();
    let pad = 36.0;
    let radius = ((width / 2.0) - pad).clamp(88.0, 132.0);
    let center_y = pad + radius;
    let height = center_y + 22.0;
    let (rect, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
    let center = pos2(rect.center().x, rect.top() + center_y);
    let painter = ui.painter_at(rect);
    let span = live.span.max(MIN_SCALE);
    let t = (live.shown / span).clamp(0.0, 1.0) as f32;

    stroke_arc(
        &painter,
        center,
        radius,
        0.0,
        1.0,
        Stroke::new(10.0, theme.gauge_track()),
    );

    let tick_n = 24;
    for i in 0..=tick_n {
        let tick_t = i as f32 / tick_n as f32;
        let inner = gauge_pos(center, radius - 7.0, tick_t);
        let outer = gauge_pos(center, radius + 1.0, tick_t);
        painter.line_segment([inner, outer], Stroke::new(1.0, theme.gauge_track()));
    }

    let has_fill = t > 0.004;
    if has_fill {
        stroke_arc(
            &painter,
            center,
            radius,
            0.0,
            t,
            Stroke::new(12.0, theme.gauge_hot(t)),
        );
    }

    let label_font = theme.ui_font(theme.text_micro);
    let marks = scale_marks(span);
    for mark in marks {
        let mark_t = (mark / span).clamp(0.0, 1.0) as f32;
        let pos = gauge_pos(center, radius + 16.0, mark_t);
        painter.text(
            pos,
            Align2::CENTER_CENTER,
            format_mark(mark),
            label_font.clone(),
            theme.muted,
        );
    }

    let num_pos = pos2(center.x, center.y - radius * 0.38);
    let unit_pos = pos2(center.x, center.y - radius * 0.14);
    let status_pos = pos2(center.x, center.y + 6.0);
    let num_size = (radius * 0.42).clamp(theme.text_gauge_min, theme.text_gauge_max);

    painter.text(
        num_pos,
        Align2::CENTER_CENTER,
        format_mbps(live.shown),
        theme.ui_font(num_size),
        theme.title,
    );
    painter.text(
        unit_pos,
        Align2::CENTER_CENTER,
        Msg::Mbps.t(),
        theme.ui_font(theme.text_small),
        theme.muted,
    );

    let (status, status_color) = status_line(live, theme);
    if status.is_empty() {
        return;
    }

    painter.text(
        status_pos,
        Align2::CENTER_CENTER,
        status,
        theme.ui_font(theme.text_small),
        status_color,
    );
}

fn status_line(live: &Live, theme: &Theme) -> (&'static str, egui::Color32) {
    match live.phase {
        Phase::Idle => ("", theme.muted),
        Phase::Connecting => (Msg::Connecting.t(), theme.muted_bright),
        Phase::Testing => (Msg::Testing.t(), theme.muted_bright),
        Phase::Ready => (Msg::Ready.t(), theme.ok),
        Phase::Failed => (Msg::Failed.t(), theme.err),
    }
}

fn stroke_arc(painter: &egui::Painter, center: Pos2, radius: f32, t0: f32, t1: f32, stroke: Stroke) {
    if (t1 - t0).abs() < 0.001 {
        return;
    }

    let span = (t1 - t0).abs();
    let n = (span * 48.0).ceil().max(8.0) as i32;
    let mut points = Vec::with_capacity((n + 1) as usize);
    for i in 0..=n {
        let t = t0 + (t1 - t0) * (i as f32 / n as f32);
        points.push(gauge_pos(center, radius, t));
    }

    painter.add(Shape::line(points, stroke));
}

fn gauge_pos(center: Pos2, radius: f32, t: f32) -> Pos2 {
    let t = t.clamp(0.0, 1.0);
    let angle = std::f32::consts::PI * (1.0 - t);
    pos2(
        center.x + radius * angle.cos(),
        center.y - radius * angle.sin(),
    )
}

fn grow_span(live: &mut Live, dt: f64) {
    let want = needed_span(live);
    if want <= live.span + settle_eps(want) {
        return;
    }

    live.span = ease_span(live.span, want, dt);
}

fn ease_shown(live: &mut Live, dt: f64) {
    let span = live.span.max(MIN_SCALE);
    let delta = live.target - live.shown;
    if delta.abs() <= settle_eps(span) {
        live.shown = live.target;
        hold_off_rim(live);
        return;
    }

    live.shown = ease_toward(live.shown, live.target, dt, SMOOTH_TAU, SWING_SECS);
    hold_off_rim(live);
}

fn hold_off_rim(live: &mut Live) {
    let want = needed_span(live);
    let expanding = want > live.span + settle_eps(want);
    if !expanding {
        return;
    }

    let rim = live.span * NEEDLE_RIM;
    if live.shown <= rim {
        return;
    }

    live.shown = rim;
}

fn ease_span(from: f64, to: f64, dt: f64) -> f64 {
    if to <= from {
        return from;
    }

    let from = from.max(MIN_SCALE);
    let ratio = (dt / SPAN_SECS).clamp(0.0, 1.0);
    let next = from * (to / from).powf(ratio);

    next.min(to)
}

fn ease_toward(from: f64, to: f64, dt: f64, tau: f64, swing_secs: f64) -> f64 {
    let delta = to - from;
    let k = 1.0 - (-dt / tau).exp();
    let eased = delta * k;
    let cap = to.abs().max(from.abs()).max(MIN_SCALE) / swing_secs * dt;
    if eased.abs() > cap {
        return from + delta.signum() * cap;
    }

    from + eased
}

fn needed_span(live: &Live) -> f64 {
    scale_max(live.target.max(live.shown)).max(live.span)
}

fn settle_eps(span: f64) -> f64 {
    span.max(MIN_SCALE) * SETTLE_FRAC
}

fn scale_max(speed: f64) -> f64 {
    let speed = if speed.is_finite() { speed.max(0.0) } else { 0.0 };
    let need = (speed * HEADROOM).max(MIN_SCALE);

    nice_ceil(need)
}

fn nice_ceil(x: f64) -> f64 {
    if !x.is_finite() || x <= 0.0 {
        return MIN_SCALE;
    }

    let exp = x.log10().floor();
    let base = 10.0_f64.powf(exp);
    let mantissa = x / base;

    if mantissa <= 1.0 {
        return base;
    }

    if mantissa <= 1.5 {
        return 1.5 * base;
    }

    if mantissa <= 2.0 {
        return 2.0 * base;
    }

    if mantissa <= 3.0 {
        return 3.0 * base;
    }

    if mantissa <= 5.0 {
        return 5.0 * base;
    }

    10.0 * base
}

fn scale_marks(max: f64) -> Vec<f64> {
    let step = nice_ceil(max / 5.0);
    let mut marks = vec![0.0];
    let mut v = step;

    loop {
        let at_end = v >= max - step * 0.05;
        if at_end {
            marks.push(max);
            return marks;
        }

        marks.push(v);
        v += step;

        if marks.len() < 8 {
            continue;
        }

        marks.push(max);
        return marks;
    }
}

fn format_mbps(v: f64) -> String {
    if !v.is_finite() || v <= 0.0 {
        return "0.000".to_owned();
    }

    if v < 1.0 {
        return format!("{v:.3}");
    }

    if v < 10.0 {
        return format!("{v:.2}");
    }

    if v < 100.0 {
        return format!("{v:.1}");
    }

    format!("{}", v.round() as i64)
}

fn format_mark(v: f64) -> String {
    if v >= 1_000_000.0 {
        let m = v / 1_000_000.0;
        return format!("{m:.0}M");
    }

    if v >= 1000.0 {
        let k = v / 1000.0;
        let whole = (k - k.round()).abs() < 0.05;
        if whole {
            return format!("{}k", k.round() as i64);
        }

        return format!("{k:.1}k");
    }

    format!("{v:.0}")
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[test]
    fn scale_grows_with_speed() {
        assert!((scale_max(0.0) - 20.0).abs() < 1e-9);
        assert!((scale_max(18.0) - 30.0).abs() < 1e-9);
        assert!((scale_max(80.0) - 100.0).abs() < 1e-9);
        assert!((scale_max(18_000.0) - 30_000.0).abs() < 1e-6);
    }

    #[test]
    fn marks_fit_the_dial() {
        assert_eq!(scale_marks(20.0), vec![0.0, 5.0, 10.0, 15.0, 20.0]);
        assert_eq!(
            scale_marks(20_000.0),
            vec![0.0, 5_000.0, 10_000.0, 15_000.0, 20_000.0]
        );
    }

    #[test]
    fn format_mark_uses_k() {
        assert_eq!(format_mark(20.0), "20");
        assert_eq!(format_mark(1_500.0), "1.5k");
        assert_eq!(format_mark(20_000.0), "20k");
    }

    #[test]
    fn format_mbps_precision() {
        assert_eq!(format_mbps(0.0), "0.000");
        assert_eq!(format_mbps(0.42), "0.420");
        assert_eq!(format_mbps(4.2), "4.20");
        assert_eq!(format_mbps(42.0), "42.0");
        assert_eq!(format_mbps(142.4), "142");
    }

    #[test]
    fn shown_takes_more_than_a_second() {
        let meter = SpeedMeter::new();
        meter.begin();
        meter.finish_ok(80.0);

        let mut now = 1.0;
        for _ in 0..20 {
            meter.tick(now);
            now += 0.05;
        }

        let shown = meter.lock().shown;
        assert!(shown > 5.0, "{shown}");
        assert!(shown < 60.0, "{shown}");
        assert!(meter.needs_repaint());
    }

    #[test]
    fn scale_jump_does_not_yank_needle_to_zero() {
        let meter = SpeedMeter::new();
        meter.begin();
        meter.on_event(SpeedEvent::Sample {
            mbps: 16.0,
            elapsed: Duration::from_millis(300),
            bytes: 1,
        });

        let mut now = 1.0;
        for _ in 0..40 {
            meter.tick(now);
            now += 0.05;
        }

        let before = {
            let live = meter.lock();
            live.shown / live.span.max(MIN_SCALE)
        };
        assert!(before > 0.4, "{before}");

        meter.on_event(SpeedEvent::Sample {
            mbps: 2_400.0,
            elapsed: Duration::from_secs(1),
            bytes: 1,
        });
        meter.tick(now);

        let after = {
            let live = meter.lock();
            live.shown / live.span.max(MIN_SCALE)
        };
        assert!(after > 0.25, "yanked to {after} from {before}");
        assert!((after - before).abs() < 0.35, "jumped {before} → {after}");
    }
}
