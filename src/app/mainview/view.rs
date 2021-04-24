use crate::view::View;
use crate::geometry::*;

use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnimationKind {
    Absolute,
    Relative,
}

impl AnimationKind {
    pub fn is_absolute(&self) -> bool {
        *self == Self::Absolute
    }

    pub fn is_relative(&self) -> bool {
        *self == Self::Relative
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AnimationOrder {
    Transform { center: Point, scale: f32 },
    Translate { center: Point },
    Scale { scale: f32 },
}

impl AnimationOrder {
    pub fn center(&self) -> Option<Point> {
        match self {
            AnimationOrder::Transform { center, .. } => Some(*center),
            AnimationOrder::Translate { center } => Some(*center),
            AnimationOrder::Scale { .. } => None,
        }
    }

    pub fn scale(&self) -> Option<f32> {
        match self {
            AnimationOrder::Transform { scale, .. } => Some(*scale),
            AnimationOrder::Translate { .. } => None,
            AnimationOrder::Scale { scale } => Some(*scale),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AnimationDef {
    kind: AnimationKind,
    order: AnimationOrder,
}


pub trait EasingFunction {
    fn value_at_normalized_time(time: f64) -> f64;
}

pub struct EasingExpoOut {}

impl EasingFunction for EasingExpoOut {
    #[inline]
    fn value_at_normalized_time(time: f64) -> f64 {
        if time <= 0.0 || time >= 1.0 {
            time
        } else {
            2.0f64.powf(10.0 * time - 10.0)
        }
    }
}

pub struct EasingElasticOut {}

impl EasingFunction for EasingElasticOut {
    fn value_at_normalized_time(time: f64) -> f64 {
        const C4: f64 = std::f64::consts::TAU / 3.0;

        if time <= 0.0 || time >= 1.0 {
            time.clamp(0.0, 1.0)
        } else {
            let expo = -10.0 * time;
            let period = (time * 10.0 - 0.75) * C4;

            2.0f64.powf(expo) * period.sin() + 1.0
        }
    }
}

pub struct EasingCirc {}

impl EasingFunction for EasingCirc {

    #[inline]
    fn value_at_normalized_time(time: f64) -> f64 {
        if time < 0.5 {
            let pow = (2.0 * time).powi(2);
            let sqrt = (1.0 - pow).sqrt();
            let num = 1.0 - sqrt;

            num / 2.0
        } else {
            let pow = (-2.0 * time + 2.0).powi(2);
            let sqrt = (1.0 - pow).sqrt();
            let num = sqrt + 1.0;

            num / 2.0
        }
    }
}

pub struct ViewLerp {
    start: View,
    end: View,

    origin_delta: Point,
    scale_delta: f32,
}

impl ViewLerp {
    pub fn new(start: View, end: View) -> Self {

        let origin_delta = end.center - start.center;
        let scale_delta = end.scale - start.scale;

        Self {
            start,
            end,
            origin_delta,
            scale_delta
        }
    }

    pub fn lerp(&self, t: f64) -> View {
        if t <= 0.0 {
            self.start
        } else if t >= 1.0 {
            self.end
        } else {
            let center = self.start.center + self.origin_delta * (t as f32);
            let scale = self.start.scale + self.scale_delta * (t as f32);
            View { center, scale }
        }
    }
}


pub struct ViewAnimation<E>
where E: EasingFunction,
{
    view_lerp: ViewLerp,
    duration: Duration,

    now: Duration,

    _easing: std::marker::PhantomData<E>,
}

impl<E: EasingFunction> ViewAnimation<E> {

    pub fn from_anim_def(start: View, anim: AnimationDef, duration: Duration) -> Self {
        let order_center = anim.order.center().unwrap_or(Point::ZERO);
        let order_scale = anim.order.scale().unwrap_or(0.0);

        let end = match anim.kind {
            AnimationKind::Absolute => {
                View { center: order_center,
                       scale: order_scale }
            }
            AnimationKind::Relative => {
                View { center: start.center + order_center,
                       scale: start.scale + order_scale }
            }
        };

        let view_lerp = ViewLerp::new(start, end);

        let now = Duration::new(0, 0);

        Self {
            view_lerp,
            duration,

            now,

            _easing: std::marker::PhantomData,
        }
    }

    pub fn new(start: View, end: View, duration: Duration) -> Self {

        let view_lerp = ViewLerp::new(start, end);

        let now = Duration::new(0, 0);

        Self {
            view_lerp,
            duration,

            now,

            _easing: std::marker::PhantomData,
        }
    }

    pub fn view_at_time(&self, time: Duration) -> View {
        let norm_time = self.duration.as_secs_f64() / time.as_secs_f64();

        let anim_time = E::value_at_normalized_time(norm_time);

        self.view_lerp.lerp(anim_time)
    }

    pub fn current_view(&self) -> View {
        self.view_at_time(self.now)
    }

    pub fn update(&mut self, delta: Duration) {
        self.now += delta;
    }
}
