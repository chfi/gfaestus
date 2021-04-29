use crate::geometry::*;
use crate::view::{ScreenDims, View};

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
    // instant: bool,
}

impl AnimationDef {
    pub fn pan_key(scale: f32, h: isize, v: isize) -> Self {
        let kind = AnimationKind::Relative;

        let mult = 10.0f32;

        let center = {
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

        let center = center * (mult * scale);

        let order = AnimationOrder::Translate { center };

        Self { kind, order }
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
            time.clamp(0.0, 1.0)
        } else {
            1.0 - 2.0f64.powf(-10.0 * time)
        }
    }
}

pub struct EasingExpoIn {}

impl EasingFunction for EasingExpoIn {
    #[inline]
    fn value_at_normalized_time(time: f64) -> f64 {
        if time <= 0.0 || time >= 1.0 {
            time.clamp(0.0, 1.0)
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
            scale_delta,
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
        let duration = self.duration.as_secs_f64();

        let norm_time = if time > self.duration {
            1.0
        } else {
            time.as_secs_f64() / duration
        };

        let anim_time = (self.easing)(norm_time);

        self.view_lerp.lerp(anim_time)
    }

    pub fn current_view(&self) -> View {
        self.view_at_time(self.now)
    }

    pub fn update(&mut self, delta: Duration) {
        self.now += delta;
    }

    pub fn is_done(&self) -> bool {
        self.now >= self.duration
    }
}

pub struct ViewAnimation<E>
where
    E: EasingFunction,
{
    view_lerp: ViewLerp,
    duration: Duration,

    now: Duration,

    _easing: std::marker::PhantomData<E>,
}

impl<E: EasingFunction> ViewAnimation<E> {
    pub fn from_anim_def(start: View, anim: AnimationDef, duration: Duration) -> Self {
        let end = match anim.kind {
            AnimationKind::Absolute => View {
                center: anim.order.center().unwrap_or(start.center),
                scale: anim.order.scale().unwrap_or(start.scale),
            },
            AnimationKind::Relative => View {
                center: start.center + anim.order.center().unwrap_or(Point::ZERO),
                scale: start.scale + anim.order.scale().unwrap_or(0.0),
            },
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
        let norm_time = if time >= self.duration {
            1.0
        } else {
            time.as_secs_f64() / self.duration.as_secs_f64()
        };

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

use crossbeam::atomic::AtomicCell;
use crossbeam::channel;
use std::sync::Arc;

pub struct AnimHandlerNew {
    // settings: Arc<AtomicCell<AnimSettings>>,
    screen_dims: Arc<AtomicCell<ScreenDims>>,
    initial_view: Arc<AtomicCell<View>>,
    mouse_pos: Arc<AtomicCell<Point>>,

    _join_handle: std::thread::JoinHandle<()>,
    anim_tx: channel::Sender<AnimationDef>,
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
                    let view_anim: ViewAnimation<EasingExpoOut> =
                        ViewAnimation::from_anim_def(cur_view, def, Duration::from_millis(100));

                    animation = Some(view_anim.boxed());
                    last_update = Instant::now();
                }

                let now = Instant::now();

                let delta: Duration = now.duration_since(last_update);

                if delta >= update_delay {
                    let mut anim_done = false;

                    if let Some(anim) = animation.as_mut() {
                        println!("delta: {:?}", delta);
                        anim.update(delta);

                        view.store(anim.current_view());

                        anim_done = anim.is_done();
                    }

                    if anim_done {
                        animation.take();
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

    pub fn send_anim_def(&self, anim_def: AnimationDef) {
        self.anim_tx.send(anim_def).unwrap();
    }

    pub fn pan_key(&self, scale: f32, up: bool, right: bool, down: bool, left: bool) {
        let h = if right {
            1
        } else if left {
            -1
        } else {
            0
        };

        let v = if up {
            -1
        } else if down {
            1
        } else {
            0
        };

        let anim_def = AnimationDef::pan_key(scale, h, v);

        self.anim_tx.send(anim_def).unwrap();
    }
}

#[derive(Debug, Default, Clone)]
pub struct KeyPanState {
    up: Arc<AtomicCell<bool>>,
    right: Arc<AtomicCell<bool>>,
    down: Arc<AtomicCell<bool>>,
    left: Arc<AtomicCell<bool>>,
    drift: Arc<AtomicCell<Option<Point>>>,
}

impl KeyPanState {
    pub fn up(&self) -> bool {
        self.up.load()
    }

    pub fn right(&self) -> bool {
        self.right.load()
    }

    pub fn down(&self) -> bool {
        self.down.load()
    }

    pub fn left(&self) -> bool {
        self.left.load()
    }

    pub fn active(&self) -> bool {
        self.up() || self.right() || self.down() || self.left()
    }

    pub fn animation_def(&self, scale: f32) -> Option<AnimationDef> {
        if !self.active() {
            return None;
        }

        let kind = AnimationKind::Relative;

        if let Some(center) = self.drift.load() {
            // let center = self.drift.load().unwrap_or_default();

            let order = AnimationOrder::Translate { center };

            return Some(AnimationDef { kind, order });
        }

        let d_x = match (self.left(), self.right()) {
            (true, false) => -1.0,
            (false, true) => 1.0,
            _ => 0.0,
        };

        let d_y = match (self.up(), self.down()) {
            (true, false) => -1.0,
            (false, true) => 1.0,
            _ => 0.0,
        };

        let mult = 10.0;

        let center = Point::new(d_x * mult * scale, d_y * mult * scale);

        let order = AnimationOrder::Translate { center };

        Some(AnimationDef { kind, order })
    }

    pub fn reset(&mut self) {
        self.up.store(false);
        self.right.store(false);
        self.down.store(false);
        self.left.store(false);

        self.drift.store(None);
    }

    pub fn set_up(&self, pressed: bool) {
        self.up.store(pressed);
    }

    pub fn set_right(&self, pressed: bool) {
        self.right.store(pressed);
    }

    pub fn set_down(&self, pressed: bool) {
        self.down.store(pressed);
    }

    pub fn set_left(&self, pressed: bool) {
        self.left.store(pressed);
    }

    pub fn drift(&self) {
        let d_x = match (self.left(), self.right()) {
            (true, false) => -1.0,
            (false, true) => 1.0,
            _ => 0.0,
        };

        let d_y = match (self.up(), self.down()) {
            (true, false) => -1.0,
            (false, true) => 1.0,
            _ => 0.0,
        };

        if d_x != 0.0 && d_y != 0.0 {
            self.drift.store(Some(Point::new(d_x, d_y)));
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MousePanState {
    Inactive,
    Continuous {
        mouse_screen_origin: Point,
    },
    ClickAndDrag {
        view_center_start: Point,
        mouse_world_origin: Point,
    },
}

impl std::default::Default for MousePanState {
    fn default() -> Self {
        Self::Inactive
    }
}

impl MousePanState {
    pub fn active(&self) -> bool {
        *self != Self::Inactive
    }

    pub fn animation_def<D: Into<ScreenDims>>(
        &self,
        scale: f32,
        screen_dims: D,
        cur_mouse_screen: Point,
        cur_mouse_world: Point,
    ) -> Option<AnimationDef> {
        match self {
            MousePanState::Inactive => None,
            MousePanState::Continuous {
                mouse_screen_origin,
            } => {
                let dims = screen_dims.into();

                let mouse_delta = cur_mouse_screen - mouse_screen_origin;

                let mouse_norm = Point {
                    x: mouse_delta.x / dims.width,
                    y: mouse_delta.y / dims.height,
                };

                let center = mouse_norm * scale;

                let kind = AnimationKind::Relative;
                let order = AnimationOrder::Translate { center };

                Some(AnimationDef { order, kind })
            }
            MousePanState::ClickAndDrag {
                view_center_start,
                mouse_world_origin,
            } => {
                let mouse_delta = *mouse_world_origin - cur_mouse_world;

                let center = *view_center_start + mouse_delta;

                let kind = AnimationKind::Absolute;
                let order = AnimationOrder::Translate { center };

                Some(AnimationDef { order, kind })
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct ViewInputState {
    pub key_pan: KeyPanState,

    pub mouse_pan: Arc<AtomicCell<MousePanState>>,
}

impl std::default::Default for ViewInputState {
    fn default() -> Self {
        Self {
            key_pan: Default::default(),
            mouse_pan: Arc::new(AtomicCell::new(MousePanState::Inactive)),
        }
    }
}

impl ViewInputState {
    pub fn animation_def<D: Into<ScreenDims>>(
        &self,
        view: View,
        screen_dims: D,
        cur_mouse_screen: Point,
        cur_mouse_world: Point,
    ) -> Option<AnimationDef> {
        let mouse_pan = self.mouse_pan.load();

        if mouse_pan.active() {
            mouse_pan.animation_def(view.scale, screen_dims, cur_mouse_screen, cur_mouse_world)
        } else {
            self.key_pan.animation_def(view.scale)
        }
    }

    pub fn start_mouse_pan(&self, view: View, screen_mouse_pos: Point) {
        let pan_state = MousePanState::Continuous {
            mouse_screen_origin: screen_mouse_pos,
        };

        self.mouse_pan.store(pan_state);
    }

    pub fn start_click_and_drag_pan(&self, view: View, world_mouse_pos: Point) {
        let pan_state = MousePanState::ClickAndDrag {
            view_center_start: view.center,
            mouse_world_origin: world_mouse_pos,
        };

        self.mouse_pan.store(pan_state);
    }

    pub fn mouse_released(&self) {
        self.mouse_pan.store(MousePanState::Inactive);
    }
}
