// rgb ctrl
// Copyright (C) 2025 mari
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 2 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <http://www.gnu.org/licenses/>.

use openrgb2::{Color, Controller, OpenRgbClient};
use rand::Rng;
use std::collections::VecDeque;
use std::fs::File;
use std::io::Read;
use std::mem::MaybeUninit;
use std::slice;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::time::interval;

// --- CONFIGURATION ---
const GRID_WIDTH: usize = 22;
const GRID_HEIGHT: usize = 6;
const TICK_RATE_MS: u64 = 30;
const INPUT_DEVICE_PATH: &str = "/dev/input/event9";

// --- KEY CODES ---
const EV_KEY: u16 = 1;
const KEY_W: u16 = 17;
const KEY_A: u16 = 30;
const KEY_S: u16 = 31;
const KEY_D: u16 = 32;
const KEY_UP: u16 = 103;
const KEY_DOWN: u16 = 108;
const KEY_LEFT: u16 = 105;
const KEY_RIGHT: u16 = 106;

// --- RAW INPUT STRUCTS ---
#[derive(Debug)]
#[repr(C)]
struct InputEvent {
    time_sec: i64,
    time_usec: i64,
    type_: u16,
    code: u16,
    value: i32,
}

impl InputEvent {
    fn new_uninit() -> MaybeUninit<Self> {
        MaybeUninit::uninit()
    }
}

// --- UTILS ---
fn key_to_grid(code: u16) -> (i32, i32) {
    match code {
        1 => (0, 0),                            // Esc
        59..=68 => ((code - 59 + 2) as i32, 0), // F1-F10
        41 => (0, 1),                           // Grave
        2..=13 => ((code - 1) as i32, 1),       // 1 through =
        15 => (0, 2),                           // Tab
        16..=25 => ((code - 15) as i32, 2),     // Q through P
        58 => (0, 3),                           // Caps
        30..=38 => ((code - 29) as i32, 3),     // A through L
        42 => (0, 4),                           // LShift
        44..=50 => ((code - 43) as i32, 4),     // Z through M
        29 => (0, 5),                           // LCtrl
        103 => (19, 4),                         // Up
        108 => (19, 5),                         // Down
        105 => (18, 5),                         // Left
        106 => (20, 5),                         // Right
        57 => (10, 5),                          // Space
        _ => (
            rand::rng().random_range(0..GRID_WIDTH as i32),
            rand::rng().random_range(0..GRID_HEIGHT as i32),
        ),
    }
}

// --- STATE MACHINE ---
#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Ambient,
    Snake,
    GameOver,
}

#[derive(Clone, Copy, PartialEq)]
struct Point {
    x: i32,
    y: i32,
}

struct Ripple {
    x: f32,
    y: f32,
    age: f32,
    max_age: f32,
}

struct AppState {
    mode: Mode,
    width: i32,
    height: i32,

    input_history: VecDeque<u16>,
    snake: Vec<Point>,
    food: Point,
    direction: Point,
    snake_timer: u64,
    last_snake_update: Instant,
    ripples: Vec<Ripple>,
    time_tick: f32,
    game_over_timer: Option<Instant>,
}

impl AppState {
    fn new(w: i32, h: i32) -> Self {
        Self {
            mode: Mode::Ambient,
            width: w,
            height: h,
            input_history: VecDeque::with_capacity(10),
            snake: vec![],
            food: Point { x: 0, y: 0 },
            direction: Point { x: 1, y: 0 },
            snake_timer: 150,
            last_snake_update: Instant::now(),
            ripples: Vec::new(),
            time_tick: 0.0,
            game_over_timer: None,
        }
    }

    fn reset_snake(&mut self) {
        self.snake = vec![
            Point { x: 5, y: 3 },
            Point { x: 4, y: 3 },
            Point { x: 3, y: 3 },
        ];
        self.direction = Point { x: 1, y: 0 };
        self.spawn_food();
        self.mode = Mode::Snake;
    }

    fn spawn_food(&mut self) {
        let mut rng = rand::rng();
        loop {
            let x = rng.random_range(0..self.width);
            let y = rng.random_range(0..self.height);
            let p = Point { x, y };
            if !self.snake.contains(&p) {
                self.food = p;
                break;
            }
        }
    }

    fn handle_input(&mut self, code: u16) {
        if self.input_history.len() >= 6 {
            self.input_history.pop_front();
        }
        self.input_history.push_back(code);

        let seq = [KEY_UP, KEY_DOWN, KEY_LEFT, KEY_RIGHT, KEY_UP, KEY_DOWN];
        if self.input_history.iter().eq(seq.iter()) {
            println!(">>> CHEAT CODE: SNAKE MODE <<<");
            self.reset_snake();
            self.input_history.clear();
            return;
        }

        match self.mode {
            Mode::Ambient => {
                let (gx, gy) = key_to_grid(code);
                self.ripples.push(Ripple {
                    x: gx as f32,
                    y: gy as f32,
                    age: 0.0,
                    max_age: 12.0,
                });
            }
            Mode::Snake => {
                let new_dir = match code {
                    KEY_UP | KEY_W if self.direction.y != 1 => Some(Point { x: 0, y: -1 }),
                    KEY_DOWN | KEY_S if self.direction.y != -1 => Some(Point { x: 0, y: 1 }),
                    KEY_LEFT | KEY_A if self.direction.x != 1 => Some(Point { x: -1, y: 0 }),
                    KEY_RIGHT | KEY_D if self.direction.x != -1 => Some(Point { x: 1, y: 0 }),
                    _ => None,
                };
                if let Some(d) = new_dir {
                    self.direction = d;
                }
            }
            Mode::GameOver => {}
        }
    }

    fn update(&mut self) {
        match self.mode {
            Mode::Ambient => {
                self.time_tick += 0.15;
                for r in &mut self.ripples {
                    r.age += 1.0;
                }
                self.ripples.retain(|r| r.age < r.max_age);
            }
            Mode::Snake => {
                if self.last_snake_update.elapsed() >= Duration::from_millis(self.snake_timer) {
                    self.step_snake();
                    self.last_snake_update = Instant::now();
                }
            }
            Mode::GameOver => {
                if let Some(timer) = self.game_over_timer
                    && timer.elapsed() >= Duration::from_secs(5)
                {
                    self.mode = Mode::Ambient;
                    self.game_over_timer = None;
                }
            }
        }
    }

    fn step_snake(&mut self) {
        let head = self.snake[0];
        let new_head = Point {
            x: head.x + self.direction.x,
            y: head.y + self.direction.y,
        };

        if new_head.x < 0
            || new_head.x >= self.width
            || new_head.y < 0
            || new_head.y >= self.height
            || self.snake.contains(&new_head)
        {
            self.mode = Mode::GameOver;
            self.game_over_timer = Some(Instant::now());
            return;
        }

        self.snake.insert(0, new_head);
        if new_head == self.food {
            self.spawn_food();
            if self.snake_timer > 50 {
                self.snake_timer -= 2;
            }
        } else {
            self.snake.pop();
        }
    }

    fn get_water_base(&self, x: f32, y: f32) -> Color {
        let t = self.time_tick;
        let wave1 = ((x * 0.4) + (y * 0.4) + t).sin();
        let wave2 = ((x * 0.6) - (t * 1.5)).cos();
        let wave3 = ((y * 0.5) + (t * 0.5)).sin();
        let combined = (wave1 + wave2 + wave3) / 3.0;

        let brightness = 0.2 + (0.5 * combined);

        // Bluish Snow Palette
        let r = (brightness * 200.0) as u8;
        let g = (brightness * 220.0) as u8;
        let b = (brightness * 255.0) as u8;

        Color::new(r, g, b)
    }

    fn get_keyboard_color(&self, x: i32, y: i32) -> Color {
        match self.mode {
            Mode::Ambient => {
                let mut base = self.get_water_base(x as f32, y as f32);

                for r in &self.ripples {
                    let rdx = x as f32 - r.x;
                    let rdy = y as f32 - r.y;
                    let r_dist = (rdx * rdx + rdy * rdy).sqrt();
                    let radius = r.age * 1.2;
                    let width = 1.5;

                    if (r_dist - radius).abs() < width {
                        let fade = 1.0 - (r.age / r.max_age).powf(3.0);
                        if fade > 0.0 {
                            base.r = base.r.saturating_add((fade * 255.0) as u8);
                            base.g = base.g.saturating_add((fade * 255.0) as u8);
                            base.b = base.b.saturating_add((fade * 255.0) as u8);
                        }
                    }
                }
                base
            }
            Mode::Snake => {
                let p = Point { x, y };
                if self.snake.contains(&p) {
                    if self.snake[0] == p {
                        return Color::new(0, 255, 0);
                    }
                    return Color::new(0, 150, 0);
                }
                if self.food == p {
                    return Color::new(255, 0, 255);
                }
                Color::new(5, 5, 5)
            }
            Mode::GameOver => {
                let elapsed_ms = self
                    .game_over_timer
                    .map(|t| t.elapsed().as_millis())
                    .unwrap_or(0);
                if (elapsed_ms / 250).is_multiple_of(2) {
                    Color::new(255, 0, 0)
                } else {
                    Color::new(0, 0, 0)
                }
            }
        }
    }

    // UPDATED: High-Floor Brightness & Slower Animation
    fn get_ram_color(&self, stick_idx: usize, led_idx: usize, total_leds: usize) -> Color {
        let x = stick_idx as f32;
        let y_norm = led_idx as f32 / total_leds as f32;
        let t = self.time_tick;

        // Slow drift (0.4 factor)
        let phase = (x * 0.8) + (y_norm * 4.0) + (t * 0.4);
        let wave = phase.sin() * 0.5 + 0.5; // 0.0 to 1.0

        // COIL WHINE FIX:
        // We enforce a minimum brightness floor of ~50.
        // We oscillate between 50 and 200 (approx) instead of 0 and 50.

        let min_b = 60.0;
        let max_b = 200.0;
        let min_g = 55.0;
        let max_g = 180.0;
        let min_r = 40.0;
        let max_r = 110.0; // Keep R low to prevent pink tint

        let r = (min_r + (wave * (max_r - min_r))) as u8;
        let g = (min_g + (wave * (max_g - min_g))) as u8;
        let b = (min_b + (wave * (max_b - min_b))) as u8;

        Color::new(r, g, b)
    }
}

// --- DEVICE MANAGEMENT ---
struct DeviceGroup {
    keyboards: Vec<Controller>,
    mice: Vec<Controller>,
    rams: Vec<Controller>,
    fans: Vec<Controller>,
}

impl DeviceGroup {
    fn sort<I>(controllers: I) -> Self
    where
        I: IntoIterator<Item = Controller>,
    {
        let mut group = DeviceGroup {
            keyboards: vec![],
            mice: vec![],
            rams: vec![],
            fans: vec![],
        };

        for c in controllers {
            let name = c.name().to_lowercase();
            if name.contains("keyboard") || name.contains("blackwidow") {
                group.keyboards.push(c);
            } else if name.contains("mouse") || name.contains("deathadder") {
                group.mice.push(c);
            } else if name.contains("dram")
                || name.contains("memory")
                || name.contains("ene")
                || name.contains("trident")
                || name.contains("g.skill")
                || name.contains("gigabyte")
            {
                group.rams.push(c);
            } else {
                group.fans.push(c);
            }
        }
        group
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("--- RGB DAEMON STARTED ---");

    let client = OpenRgbClient::connect().await?;
    let controllers = client.get_all_controllers().await?;

    println!("Initializing devices...");
    for c in &controllers {
        if let Err(e) = c.init().await {
            eprintln!("Warning: Failed to init device '{}': {}", c.name(), e);
        }
    }

    let devices = DeviceGroup::sort(controllers);

    println!("Found Devices:");
    println!("  Keyboards: {}", devices.keyboards.len());
    println!("  Mice:      {}", devices.mice.len());
    println!("  RAM:       {}", devices.rams.len());
    println!("  Fans/Misc: {}", devices.fans.len());

    let app_state = Arc::new(Mutex::new(AppState::new(
        GRID_WIDTH as i32,
        GRID_HEIGHT as i32,
    )));

    // --- INPUT TASK ---
    let input_state = app_state.clone();
    tokio::task::spawn_blocking(move || {
        let mut f = match File::open(INPUT_DEVICE_PATH) {
            Ok(f) => f,
            Err(e) => {
                eprintln!("CRITICAL: Could not open input: {}", e);
                return;
            }
        };

        loop {
            let mut event_buffer = InputEvent::new_uninit();
            unsafe {
                let slice = slice::from_raw_parts_mut(
                    event_buffer.as_mut_ptr() as *mut u8,
                    std::mem::size_of::<InputEvent>(),
                );
                if f.read_exact(slice).is_err() {
                    break;
                }
            }

            let event = unsafe { event_buffer.assume_init() };
            if event.type_ == EV_KEY && event.value == 1 {
                let mut state = input_state.lock().unwrap();
                state.handle_input(event.code);
            }
        }
    });

    // --- RENDER LOOP ---
    let mut ticker = interval(Duration::from_millis(TICK_RATE_MS));
    let mut tick_count: u64 = 0;

    loop {
        ticker.tick().await;
        tick_count += 1;

        {
            let mut state = app_state.lock().unwrap();
            state.update();
        }

        let state = app_state.lock().unwrap();

        // 1. UPDATE KEYBOARDS
        for kb in &devices.keyboards {
            let mut leds = Vec::with_capacity(kb.num_leds());
            for y in 0..GRID_HEIGHT {
                for x in 0..GRID_WIDTH {
                    leds.push(state.get_keyboard_color(x as i32, y as i32));
                }
            }
            let target_len = kb.num_leds();
            if leds.len() < target_len {
                leds.resize(target_len, Color::new(0, 0, 0));
            }
            if leds.len() > target_len {
                leds.truncate(target_len);
            }

            let _ = kb.set_leds(leds).await;
        }

        // 2. UPDATE RAM (Throttled)
        if tick_count.is_multiple_of(3) {
            for (i, ram) in devices.rams.iter().enumerate() {
                let count = ram.num_leds();
                let mut leds = Vec::with_capacity(count);
                for led_idx in 0..count {
                    leds.push(state.get_ram_color(i, led_idx, count));
                }
                let _ = ram.set_leds(leds).await;
            }
        }

        // 3. UPDATE MOUSE
        for mouse in &devices.mice {
            let base_color = state.get_water_base(10.0, 3.0);
            let count = mouse.num_leds();
            let leds = vec![base_color; count];
            let _ = mouse.set_leds(leds).await;
        }

        // 4. UPDATE FANS (Force Off)
        for fan in &devices.fans {
            let count = fan.num_leds();
            let leds = vec![Color::new(0, 0, 0); count];
            let _ = fan.set_leds(leds).await;
        }
    }
}
