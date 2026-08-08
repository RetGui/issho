use std::{error::Error, io, num::NonZeroU32, rc::Rc};

use issho::{
    AccessKey, AccessNode, AccessRect, AccessTree, LiveSetting, Role, SupportedTextSelection,
};
use softbuffer::{Context, Surface};
use vello_cpu::{Pixmap, RenderContext, Resources, color::palette::css, kurbo::Rect};
use winit::{
    application::ApplicationHandler,
    dpi::{PhysicalPosition, PhysicalSize},
    event::{ElementState, MouseButton, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop},
    keyboard::{Key, NamedKey},
    window::{Window, WindowId},
};

const WINDOW_WIDTH: u32 = 480;
const WINDOW_HEIGHT: u32 = 180;
const MIN_WINDOW_WIDTH: u32 = 360;
const MIN_WINDOW_HEIGHT: u32 = 140;
const MAX_WINDOW_SIZE: u32 = 4096;
const PADDING: f64 = 20.0;
const BUTTON_WIDTH: f64 = 100.0;

#[derive(Clone, Copy)]
struct CounterNodes {
    root: AccessKey,
    decrement: AccessKey,
    value: AccessKey,
    increment: AccessKey,
}

#[derive(Clone, Copy)]
struct CounterLayout {
    root: AccessRect,
    decrement: AccessRect,
    value: AccessRect,
    increment: AccessRect,
}

impl CounterLayout {
    fn new(size: PhysicalSize<u32>) -> Self {
        let width = f64::from(size.width);
        let height = f64::from(size.height);
        let content_height = (height - PADDING * 2.0).max(0.0);
        let value_width = (width - PADDING * 4.0 - BUTTON_WIDTH * 2.0).max(0.0);

        Self {
            root: AccessRect::new(0.0, 0.0, width, height),
            decrement: AccessRect::new(PADDING, PADDING, BUTTON_WIDTH, content_height),
            value: AccessRect::new(
                PADDING * 2.0 + BUTTON_WIDTH,
                PADDING,
                value_width,
                content_height,
            ),
            increment: AccessRect::new(
                width - PADDING - BUTTON_WIDTH,
                PADDING,
                BUTTON_WIDTH,
                content_height,
            ),
        }
    }
}

struct VelloRenderer {
    surface: Surface<Rc<Window>, Rc<Window>>,
    _context: Context<Rc<Window>>,
    render_context: RenderContext,
    resources: Resources,
    pixmap: Pixmap,
}

impl VelloRenderer {
    fn new(window: Rc<Window>) -> Result<Self, Box<dyn Error>> {
        let context = Context::new(window.clone())?;
        let surface = Surface::new(&context, window)?;

        Ok(Self {
            surface,
            _context: context,
            render_context: RenderContext::new(1, 1),
            resources: Resources::new(),
            pixmap: Pixmap::new(1, 1),
        })
    }

    fn draw(&mut self, window: &Window, count: i32) -> Result<(), Box<dyn Error>> {
        let size = window.inner_size();
        let (Some(surface_width), Some(surface_height)) =
            (NonZeroU32::new(size.width), NonZeroU32::new(size.height))
        else {
            return Ok(());
        };
        let width = u16::try_from(size.width)
            .map_err(|_| io::Error::other("window width exceeds Vello CPU's u16 limit"))?;
        let height = u16::try_from(size.height)
            .map_err(|_| io::Error::other("window height exceeds Vello CPU's u16 limit"))?;

        self.surface.resize(surface_width, surface_height)?;

        if self.render_context.width() != width || self.render_context.height() != height {
            self.render_context = RenderContext::new(width, height);
            self.resources = Resources::new();
            self.pixmap.resize(width, height);
        } else {
            self.render_context.reset();
        }

        draw_counter(&mut self.render_context, CounterLayout::new(size), count);
        self.render_context.flush();
        self.render_context
            .render(&mut self.pixmap, &mut self.resources);

        let mut buffer = self.surface.buffer_mut()?;
        for (target, source) in buffer.iter_mut().zip(self.pixmap.data()) {
            *target = u32::from(source.b) | u32::from(source.g) << 8 | u32::from(source.r) << 16;
        }
        window.pre_present_notify();
        buffer.present()?;

        Ok(())
    }
}

#[derive(Clone)]
struct CounterNodeContext;

struct App {
    window: Option<Rc<Window>>,
    renderer: Option<VelloRenderer>,
    access_tree: AccessTree<Rc<Window>, CounterNodeContext>,
    nodes: Option<CounterNodes>,
    cursor_position: PhysicalPosition<f64>,
    count: i32,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let attributes = Window::default_attributes()
            .with_title("Counter")
            .with_inner_size(PhysicalSize::new(WINDOW_WIDTH, WINDOW_HEIGHT))
            .with_min_inner_size(PhysicalSize::new(MIN_WINDOW_WIDTH, MIN_WINDOW_HEIGHT))
            .with_max_inner_size(PhysicalSize::new(MAX_WINDOW_SIZE, MAX_WINDOW_SIZE));
        let window = Rc::new(
            event_loop
                .create_window(attributes)
                .expect("failed to create window"),
        );

        let mut root = AccessNode::new();
        root.set_role(Role::Window);
        root.set_name("Counter");
        let root = self.access_tree.insert_node(root, None);

        let mut decrement = AccessNode::new();
        decrement.set_role(Role::Button);
        decrement.set_name("Decrement");
        let decrement = self.access_tree.insert_node(decrement, Some(root));

        let mut value = AccessNode::new();
        value.set_role(Role::Label);
        value.set_text_supported_text_selection(SupportedTextSelection::Single);
        value.set_name("0");
        value.set_live_setting(LiveSetting::Assertive);
        let value = self.access_tree.insert_node(value, Some(root));

        let mut increment = AccessNode::new();
        increment.set_role(Role::Button);
        increment.set_name("Increment");
        let increment = self.access_tree.insert_node(increment, Some(root));
        let nodes = CounterNodes {
            root,
            decrement,
            value,
            increment,
        };

        self.renderer = Some(
            VelloRenderer::new(window.clone()).expect("failed to create the CPU rendering surface"),
        );
        self.window = Some(window.clone());
        self.nodes = Some(nodes);
        self.access_tree.set_root_window(root, window.clone());
        self.update_accessibility_bounds();
        self.update_title();
        window.request_redraw();

        // The root must be associated with the window before the native
        // platform starts responding to accessibility queries for it.
        self.access_tree.register_window(window);
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        if !self
            .window
            .as_ref()
            .is_some_and(|window| window.id() == window_id)
        {
            return;
        }

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Focused(focused) => {
                if let Some(nodes) = self.nodes {
                    if focused {
                        self.access_tree.set_focus(nodes.root, Some(nodes.root));
                    } else {
                        self.access_tree.set_focus(nodes.root, None);
                    }
                }
            }
            WindowEvent::Resized(_) => {
                self.update_accessibility_bounds();
                self.request_redraw();
            }
            WindowEvent::RedrawRequested => {
                if let Err(error) = self.draw() {
                    eprintln!("failed to draw counter: {error}");
                    event_loop.exit();
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor_position = position;
            }
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Left,
                ..
            } => self.handle_click(),
            WindowEvent::KeyboardInput { event, .. }
                if event.state == ElementState::Pressed && !event.repeat =>
            {
                match event.logical_key {
                    Key::Named(NamedKey::ArrowDown) => self.change_count(-1),
                    Key::Named(NamedKey::ArrowUp) => self.change_count(1),
                    Key::Named(NamedKey::Enter) | Key::Named(NamedKey::Space) => {
                        self.activate_focused()
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
}

impl App {
    fn new() -> Self {
        let access_tree = AccessTree::new();
        access_tree.set_framework_name("Issho");
        access_tree.set_native_platform();

        Self {
            window: None,
            renderer: None,
            access_tree,
            nodes: None,
            cursor_position: PhysicalPosition::new(0.0, 0.0),
            count: 0,
        }
    }

    fn change_count(&mut self, amount: i32) {
        self.count = self.count.saturating_add(amount);

        if let Some(nodes) = self.nodes {
            let focused_node = if amount < 0 {
                nodes.decrement
            } else {
                nodes.increment
            };
            self.access_tree.set_focus(nodes.root, Some(focused_node));
            self.access_tree
                .set_name(nodes.value, self.count.to_string());
        }

        self.update_title();
        self.request_redraw();
    }

    fn activate_focused(&mut self) {
        let Some(nodes) = self.nodes else {
            return;
        };

        match self.access_tree.get_focus(nodes.root) {
            Some(focused) if focused == nodes.decrement => self.change_count(-1),
            Some(focused) if focused == nodes.increment => self.change_count(1),
            _ => {}
        }
    }

    fn handle_click(&mut self) {
        let Some(window) = &self.window else {
            return;
        };
        let layout = CounterLayout::new(window.inner_size());

        if layout
            .decrement
            .contains(self.cursor_position.x, self.cursor_position.y)
        {
            self.change_count(-1);
        } else if layout
            .increment
            .contains(self.cursor_position.x, self.cursor_position.y)
        {
            self.change_count(1);
        }
    }

    fn draw(&mut self) -> Result<(), Box<dyn Error>> {
        let Some(window) = self.window.clone() else {
            return Ok(());
        };
        let Some(renderer) = &mut self.renderer else {
            return Ok(());
        };

        renderer.draw(&window, self.count)
    }

    fn request_redraw(&self) {
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }

    fn update_title(&self) {
        if let Some(window) = &self.window {
            window.set_title(&format!(
                "Counter: {}  |  Down/left click: -  Up/right click: +",
                self.count
            ));
        }
    }

    fn update_accessibility_bounds(&self) {
        let (Some(window), Some(nodes)) = (&self.window, self.nodes) else {
            return;
        };

        let layout = CounterLayout::new(window.inner_size());

        self.access_tree
            .get_node_mut(nodes.root)
            .expect("root node not found")
            .set_bounding_rect(layout.root);
        self.access_tree
            .get_node_mut(nodes.decrement)
            .expect("decrement node not found")
            .set_bounding_rect(layout.decrement);
        self.access_tree
            .get_node_mut(nodes.value)
            .expect("value node not found")
            .set_bounding_rect(layout.value);
        self.access_tree
            .get_node_mut(nodes.increment)
            .expect("increment node not found")
            .set_bounding_rect(layout.increment);
    }
}

fn draw_counter(context: &mut RenderContext, layout: CounterLayout, count: i32) {
    context.set_paint(css::WHITE_SMOKE);
    context.fill_rect(&to_vello_rect(layout.root));

    context.set_paint(css::LIGHT_GRAY);
    context.fill_rect(&to_vello_rect(layout.value));

    context.set_paint(css::DARK_SLATE_BLUE);
    context.fill_rect(&to_vello_rect(layout.decrement));
    context.fill_rect(&to_vello_rect(layout.increment));

    context.set_paint(css::WHITE);
    draw_button_symbol(context, layout.decrement, false);
    draw_button_symbol(context, layout.increment, true);

    context.set_paint(css::MIDNIGHT_BLUE);
    draw_seven_segment_number(context, layout.value, count);
}

fn draw_button_symbol(context: &mut RenderContext, rect: AccessRect, plus: bool) {
    let center_x = rect.x + rect.width / 2.0;
    let center_y = rect.y + rect.height / 2.0;
    let length = rect.width.min(rect.height) * 0.4;
    let thickness = (length * 0.16).max(4.0);

    context.fill_rect(&Rect::new(
        center_x - length / 2.0,
        center_y - thickness / 2.0,
        center_x + length / 2.0,
        center_y + thickness / 2.0,
    ));

    if plus {
        context.fill_rect(&Rect::new(
            center_x - thickness / 2.0,
            center_y - length / 2.0,
            center_x + thickness / 2.0,
            center_y + length / 2.0,
        ));
    }
}

fn draw_seven_segment_number(context: &mut RenderContext, rect: AccessRect, value: i32) {
    let text = value.to_string();
    let character_count = text.chars().count() as f64;
    let available_width = (rect.width - 24.0).max(1.0);
    let available_height = (rect.height - 24.0).max(1.0);
    let mut digit_height = available_height.min(84.0);
    let mut digit_width = digit_height * 0.55;
    let mut gap = digit_width * 0.22;
    let mut total_width = character_count * digit_width + (character_count - 1.0).max(0.0) * gap;

    if total_width > available_width {
        let scale = available_width / total_width;
        digit_height *= scale;
        digit_width *= scale;
        gap *= scale;
        total_width = available_width;
    }

    let mut x = rect.x + (rect.width - total_width) / 2.0;
    let y = rect.y + (rect.height - digit_height) / 2.0;

    for character in text.chars() {
        if character == '-' {
            draw_digit_segments(context, x, y, digit_width, digit_height, 0b100_0000);
        } else if let Some(digit) = character.to_digit(10) {
            const SEGMENTS: [u8; 10] = [
                0b011_1111, 0b000_0110, 0b101_1011, 0b100_1111, 0b110_0110, 0b110_1101, 0b111_1101,
                0b000_0111, 0b111_1111, 0b110_1111,
            ];
            draw_digit_segments(
                context,
                x,
                y,
                digit_width,
                digit_height,
                SEGMENTS[digit as usize],
            );
        }
        x += digit_width + gap;
    }
}

fn draw_digit_segments(
    context: &mut RenderContext,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    segments: u8,
) {
    let thickness = (width * 0.18).max(1.0);
    let middle_y = y + height / 2.0;
    let horizontal_width = (width - thickness * 2.0).max(1.0);
    let vertical_height = (height / 2.0 - thickness * 1.5).max(1.0);
    let horizontal = |segment_y| {
        Rect::new(
            x + thickness,
            segment_y,
            x + thickness + horizontal_width,
            segment_y + thickness,
        )
    };
    let vertical = |segment_x, segment_y| {
        Rect::new(
            segment_x,
            segment_y,
            segment_x + thickness,
            segment_y + vertical_height,
        )
    };

    let segment_rects = [
        horizontal(y),
        vertical(x + width - thickness, y + thickness),
        vertical(x + width - thickness, middle_y + thickness / 2.0),
        horizontal(y + height - thickness),
        vertical(x, middle_y + thickness / 2.0),
        vertical(x, y + thickness),
        horizontal(middle_y - thickness / 2.0),
    ];

    for (index, rect) in segment_rects.iter().enumerate() {
        if segments & (1 << index) != 0 {
            context.fill_rect(rect);
        }
    }
}

const fn to_vello_rect(rect: AccessRect) -> Rect {
    Rect::new(rect.x, rect.y, rect.x + rect.width, rect.y + rect.height)
}

fn main() -> Result<(), Box<dyn Error>> {
    simple_logger::SimpleLogger::new().init()?;
    let event_loop = EventLoop::new()?;
    event_loop.run_app(&mut App::new())?;
    Ok(())
}
