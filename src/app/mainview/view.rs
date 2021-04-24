use crate::view::{ScreenDims, View};
use crate::geometry::*;

use std::time::{Duration, Instant};

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

impl AnimationDef {
    pub fn pan_key(h: isize, v: isize) -> Self {
        let kind = AnimationKind::Relative;

        let center  =
        {
            use std::cmp::Ordering;

            let x = match h.cmp(&0) {
                Ordering::Less => -1.0f32,
                Ordering::Equal => 0.0f32,
                Ordering::Greater => 1.0f32,
            };

            let y = match v.cmp(&0) {
                Ordering::Less => -1.0f32,
                Ordering::Equal => 0.0f32,
                Ordering::Greater => 1.0f32,
            };

            Point { x, y }
        };

        let order = AnimationOrder::Translate {
            center,
        };

        Self {
            kind,
            order
        }
    }
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

pub struct ViewAnimationBoxed {
    view_lerp: ViewLerp,
    duration: Duration,

    now: Duration,

    easing: Box<dyn Fn(f64) -> f64>,
}

impl ViewAnimationBoxed {
    pub fn view_at_time(&self, time: Duration) -> View {
        let norm_time = self.duration.as_secs_f64() / time.as_secs_f64();
        let anim_time = (self.easing)(norm_time);

        self.view_lerp.lerp(anim_time)
    }

    pub fn current_view(&self) -> View {
        self.view_at_time(self.now)
    }

    pub fn update(&mut self, delta: Duration) {
        self.now += delta;
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

    pub fn boxed(self) -> ViewAnimationBoxed {
        let view_lerp = self.view_lerp;
        let duration = self.duration;
        let now = self.now;

        let easing = Box::new(|t| E::value_at_normalized_time(t));

        ViewAnimationBoxed {
            view_lerp,
            duration,
            now,
            easing,
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

use std::sync::Arc;
use crossbeam::atomic::AtomicCell;
use crossbeam::channel;



pub struct AnimHandlerNew {
    // settings: Arc<AtomicCell<AnimSettings>>,


    screen_dims: Arc<AtomicCell<ScreenDims>>,
    initial_view: Arc<AtomicCell<View>>,
    mouse_pos: Arc<AtomicCell<Point>>,

    _join_handle: std::thread::JoinHandle<()>,
    anim_tx: channel::Sender::<AnimationDef>,

    // animation: Option<ViewAnimationBoxed>,
    // animation: Option<Box<dyn
    // mouse_pan_screen_origin: Origin<Point>,

    // animation: Option<Animation>,
}

impl AnimHandlerNew {
    pub fn new<D: Into<ScreenDims>>(
        view: Arc<AtomicCell<View>>,
        mouse_pos: Point,
        screen_dims: D,
    ) -> Self {

        let screen_dims_ = Arc::new(AtomicCell::new(screen_dims.into()));
        let screen_dims = screen_dims_.clone();

        let initial_view = view.load();
        let initial_view_ = Arc::new(AtomicCell::new(initial_view));
        let initial_view = initial_view_.clone();

        let view_ = view;

        let mouse_pos_ = Arc::new(AtomicCell::new(mouse_pos));
        let mouse_pos = mouse_pos_.clone();


        let (anim_tx, anim_rx) = channel::unbounded::<AnimationDef>();

        let _join_handle = std::thread::spawn(move || {

            let update_delay = std::time::Duration::from_millis(5);
            let sleep_delay = std::time::Duration::from_micros(2500);

            let mut animation: Option<ViewAnimationBoxed> = None;

            let view = view_;
            let mouse_pos = mouse_pos_;
            let screen_dims = screen_dims_;
            let initial_view = initial_view_;

            let mut last_update = Instant::now();

            loop {

                let cur_view = view.load();

                while let Ok(def) = anim_rx.try_recv() {
                    let view_anim: ViewAnimation<EasingCirc> = ViewAnimation::from_anim_def(cur_view, def, Duration::from_millis(1000));

                    animation = Some(view_anim.boxed());
                    last_update = Instant::now();
                }

                let now = Instant::now();

                let delta: Duration = now.duration_since(last_update);

                if delta >= update_delay {

                    if let Some(anim) = animation.as_mut() {
                        anim.update(delta);
                        view.store(anim.current_view());
                    }

                    last_update = Instant::now();
                } else {
                    std::thread::sleep(sleep_delay);
                }
            }
        });


        Self {
            screen_dims,
            initial_view,
            mouse_pos,

            _join_handle,
            anim_tx,
        }


    }

}
