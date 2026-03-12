//! SameGame — GTK4 grid match game with levels, animations, and fruit rain
#![allow(unused_imports)]
use gtk4::prelude::*;
use gtk4::{
    Application, ApplicationWindow, Box as GtkBox, Button, CssProvider,
    EventControllerMotion, GestureClick, Grid, Label,
};
use gtk4::glib;
use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::rc::Rc;
use std::time::Duration;

// ── Constants ─────────────────────────────────────────────────────────────────

const COLS: usize = 10;
const ROWS: usize = 10;
const MIN_GROUP: usize = 3;
const N_TYPES: usize = 5;

const EMOJI: [&str; N_TYPES] = ["🍎", "🍊", "🍋", "🍇", "🫐"];
const T_CLS: [&str; N_TYPES] = ["t0", "t1", "t2", "t3", "t4"];

/// Scoring: (n - 2)²
fn score_for(n: usize) -> u32 {
    (n.saturating_sub(2) as u32).pow(2)
}

// ── XorShift RNG ─────────────────────────────────────────────────────────────

struct Rng(u64);
impl Rng {
    fn new() -> Self {
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0xdeadbeef_cafebabe);
        Rng(seed ^ 0x9e3779b97f4a7c15)
    }
    fn next(&mut self) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0 as usize
    }
}

// ── Game logic ───────────────────────────────────────────────────────────────

#[derive(Clone, Copy)]
struct Game {
    grid: [[Option<usize>; COLS]; ROWS],
    score: u32,
    level: u32,
    /// Cumulative score needed for next level-up
    next_level_at: u32,
    /// Points required for this level (increases by 50 each level)
    level_requirement: u32,
    game_over: bool,
    selected_pos: Option<(usize, usize)>,
    /// Track combo (consecutive clears without a miss)
    combo: u32,
}

impl Game {
    fn new() -> Self {
        let mut g = Game {
            grid: [[None; COLS]; ROWS],
            score: 0,
            level: 1,
            next_level_at: 100,
            level_requirement: 100,
            game_over: false,
            selected_pos: None,
            combo: 0,
        };
        let mut rng = Rng::new();
        for row in 0..ROWS {
            for col in 0..COLS {
                g.grid[row][col] = Some(rng.next() % N_TYPES);
            }
        }
        g
    }

    fn flood(&self, row: usize, col: usize) -> Vec<(usize, usize)> {
        let kind = match self.grid[row][col] {
            Some(k) => k,
            None => return vec![],
        };
        let mut vis = [[false; COLS]; ROWS];
        let mut out = Vec::new();
        let mut stk = vec![(row, col)];
        while let Some((r, c)) = stk.pop() {
            if vis[r][c] || self.grid[r][c] != Some(kind) {
                continue;
            }
            vis[r][c] = true;
            out.push((r, c));
            if r > 0 {
                stk.push((r - 1, c));
            }
            if r + 1 < ROWS {
                stk.push((r + 1, c));
            }
            if c > 0 {
                stk.push((r, c - 1));
            }
            if c + 1 < COLS {
                stk.push((r, c + 1));
            }
        }
        out
    }

    /// Remove a group. Returns (count, points, leveled_up).
    fn click(&mut self, row: usize, col: usize) -> Option<(usize, u32, bool)> {
        if self.game_over {
            return None;
        }
        let group = self.flood(row, col);
        if group.len() < MIN_GROUP {
            self.combo = 0;
            return None;
        }
        for &(r, c) in &group {
            self.grid[r][c] = None;
        }
        self.gravity();

        self.combo += 1;
        // Bonus for combos: x1, x1, x1.5, x2, x2.5 ...
        let combo_mult = if self.combo >= 3 {
            1.0 + (self.combo - 2) as f64 * 0.5
        } else {
            1.0
        };
        let pts = (score_for(group.len()) as f64 * combo_mult) as u32;
        self.score += pts;

        // Check level-up
        let leveled_up = if self.score >= self.next_level_at {
            self.level += 1;
            self.level_requirement += 50;
            self.next_level_at += self.level_requirement;
            true
        } else {
            false
        };

        if !leveled_up && !self.has_move() {
            self.game_over = true;
        }
        Some((group.len(), pts, leveled_up))
    }

    fn gravity(&mut self) {
        for col in 0..COLS {
            let kept: Vec<usize> = (0..ROWS).filter_map(|r| self.grid[r][col]).collect();
            let empty = ROWS - kept.len();
            for row in 0..ROWS {
                self.grid[row][col] = if row < empty {
                    None
                } else {
                    Some(kept[row - empty])
                };
            }
        }
    }

    fn swap_tiles(&mut self, r1: usize, c1: usize, r2: usize, c2: usize) -> bool {
        let delta_r = (r1 as i32) - (r2 as i32);
        let delta_c = (c1 as i32) - (c2 as i32);
        if (delta_r.abs() + delta_c.abs()) != 1 {
            return false;
        }
        if self.grid[r1][c1].is_none()
            || self.grid[r2][c2].is_none()
            || self.grid[r1][c1] == self.grid[r2][c2]
        {
            return false;
        }
        let t1 = self.grid[r1][c1];
        let t2 = self.grid[r2][c2];
        self.grid[r1][c1] = t2;
        self.grid[r2][c2] = t1;
        self.gravity();
        true
    }

    fn has_move(&self) -> bool {
        (0..ROWS).any(|r| (0..COLS).any(|c| self.flood(r, c).len() >= MIN_GROUP))
    }

    /// Count empty cells in the grid.
    fn empty_count(&self) -> usize {
        let mut n = 0;
        for row in 0..ROWS {
            for col in 0..COLS {
                if self.grid[row][col].is_none() {
                    n += 1;
                }
            }
        }
        n
    }

    /// Fill all empty cells with random fruit (used on level-up).
    fn refill_empties(&mut self) {
        let mut rng = Rng::new();
        for row in 0..ROWS {
            for col in 0..COLS {
                if self.grid[row][col].is_none() {
                    self.grid[row][col] = Some(rng.next() % N_TYPES);
                }
            }
        }
        self.gravity();
    }
}

// ── Main Entry ───────────────────────────────────────────────────────────────

fn main() {
    let app = Application::builder()
        .application_id("com.example.samegame")
        .build();

    app.connect_activate(build_game);
    app.run();
}

pub fn build_game(app: &Application) {
    install_css();

    let builder = gtk4::Builder::from_string(include_str!("../layout.ui"));
    let win: ApplicationWindow = builder.object("main_window").expect("Window not found");
    let score_lbl: Label = builder.object("score_label").expect("Score label not found");
    let status_lbl: Label = builder.object("status_label").expect("Status label not found");
    let new_btn: Button = builder.object("new_game_button").expect("New game button not found");
    let grid_box: GtkBox = builder.object("grid_container").expect("Grid container not found");

    win.set_application(Some(app));

    let (tile_grid, buttons) = make_tile_grid();
    grid_box.append(&tile_grid);

    let state = Rc::new(RefCell::new(Game::new()));
    let btns = Rc::new(RefCell::new(buttons));
    // Guard to prevent clicks during animations
    let animating = Rc::new(Cell::new(false));

    sync_grid(&state.borrow(), &btns.borrow());
    update_score_label(&score_lbl, &state.borrow());

    for row in 0..ROWS {
        for col in 0..COLS {
            let btn = btns.borrow()[row][col].clone();
            wire_tile(
                &btn,
                row,
                col,
                &state,
                &btns,
                &score_lbl,
                &status_lbl,
                &animating,
            );
        }
    }

    let s = state.clone();
    let b = btns.clone();
    let sl = score_lbl.clone();
    let stl = status_lbl.clone();
    let anim = animating.clone();
    new_btn.connect_clicked(move |_| {
        if anim.get() {
            return;
        }
        *s.borrow_mut() = Game::new();
        sync_grid(&s.borrow(), &b.borrow());
        update_score_label(&sl, &s.borrow());
        stl.set_label("Click a group of 3+ matching tiles!");
    });

    win.present();
}

// ── UI helpers ───────────────────────────────────────────────────────────────

fn update_score_label(lbl: &Label, game: &Game) {
    lbl.set_label(&format!(
        "Lv.{} — Score: {}  (next: {})",
        game.level, game.score, game.next_level_at
    ));
}

fn combo_text(combo: u32) -> &'static str {
    match combo {
        0..=1 => "",
        2 => " 🔥 Double!",
        3 => " 🔥🔥 Triple!",
        4 => " 🔥🔥🔥 Quad!",
        _ => " 💥 MEGA COMBO!",
    }
}

// ── Grid / tile helpers ──────────────────────────────────────────────────────

fn make_tile_grid() -> (Grid, Vec<Vec<Button>>) {
    let grid = Grid::new();
    grid.set_row_spacing(4);
    grid.set_column_spacing(4);
    grid.set_halign(gtk4::Align::Center);
    grid.add_css_class("game-grid");

    let mut buttons = Vec::new();
    for row in 0..ROWS {
        let mut row_btns = Vec::new();
        for col in 0..COLS {
            let btn = Button::new();
            btn.add_css_class("tile");
            btn.set_size_request(50, 50);
            grid.attach(&btn, col as i32, row as i32, 1, 1);
            row_btns.push(btn);
        }
        buttons.push(row_btns);
    }
    (grid, buttons)
}

fn sync_grid(game: &Game, btns: &[Vec<Button>]) {
    for row in 0..ROWS {
        for col in 0..COLS {
            set_tile(&btns[row][col], game.grid[row][col]);
        }
    }
}

fn set_tile(btn: &Button, tile: Option<usize>) {
    for cls in T_CLS {
        btn.remove_css_class(cls);
    }
    btn.remove_css_class("tile-glow");
    btn.remove_css_class("tile-dim");
    btn.remove_css_class("tile-pop");
    btn.remove_css_class("tile-drop");
    btn.remove_css_class("empty");
    match tile {
        Some(k) => {
            btn.set_label(EMOJI[k]);
            btn.add_css_class(T_CLS[k]);
        }
        None => {
            btn.set_label("");
            btn.add_css_class("empty");
        }
    }
}

fn clear_highlights(btns: &[Vec<Button>]) {
    for row in btns {
        for btn in row {
            btn.remove_css_class("tile-glow");
            btn.remove_css_class("tile-dim");
        }
    }
}

// ── Animated removal + gravity ───────────────────────────────────────────────

/// Animate: pop the matched tiles, wait, then apply gravity with drop animation.
fn animate_removal(
    group: &[(usize, usize)],
    state: &Rc<RefCell<Game>>,
    btns: &Rc<RefCell<Vec<Vec<Button>>>>,
    score_lbl: &Label,
    status_lbl: &Label,
    animating: &Rc<Cell<bool>>,
    pts: u32,
    count: usize,
    combo: u32,
    leveled_up: bool,
) {
    animating.set(true);

    // Step 1: Add pop class to matched tiles
    {
        let btns_ref = btns.borrow();
        for &(r, c) in group {
            btns_ref[r][c].add_css_class("tile-pop");
        }
    }

    let s = state.clone();
    let b = btns.clone();
    let sl = score_lbl.clone();
    let stl = status_lbl.clone();
    let anim = animating.clone();

    // Step 2: After pop animation, clear tiles and apply gravity
    glib::timeout_add_local_once(Duration::from_millis(250), move || {
        {
            let game = s.borrow();
            sync_grid(&game, &b.borrow());
        }

        update_score_label(&sl, &s.borrow());
        let combo_str = combo_text(combo);
        stl.set_label(&format!("✨ Removed {} tiles  +{} pts{}", count, pts, combo_str));

        if leveled_up {
            // Level up! Show congrats, then rain fruit
            let level = s.borrow().level;
            let empty_n = s.borrow().empty_count();
            stl.set_label(&format!(
                "🎉 LEVEL {}!  +{} new fruit incoming!",
                level, empty_n
            ));

            // Refill empties in game state
            s.borrow_mut().refill_empties();

            // Animate fruit rain: reveal new tiles row by row from bottom
            animate_fruit_rain(&s, &b, &sl, &stl, &anim);
        } else if s.borrow().game_over {
            let score = s.borrow().score;
            let level = s.borrow().level;
            stl.set_label(&format!(
                "🏁 Game Over! Level {} — Final Score: {}",
                level, score
            ));
            anim.set(false);
        } else {
            anim.set(false);
        }
    });
}

/// Animate new fruit appearing row-by-row from the bottom up.
fn animate_fruit_rain(
    state: &Rc<RefCell<Game>>,
    btns: &Rc<RefCell<Vec<Vec<Button>>>>,
    score_lbl: &Label,
    status_lbl: &Label,
    animating: &Rc<Cell<bool>>,
) {
    // We reveal one row at a time from bottom (ROWS-1) to top (0),
    // with a short delay between rows for a rain effect.
    let game_snapshot = *state.borrow();
    let row_idx = Rc::new(Cell::new(ROWS as i32 - 1));

    let b = btns.clone();
    let s = state.clone();
    let sl = score_lbl.clone();
    let stl = status_lbl.clone();
    let anim = animating.clone();

    glib::timeout_add_local(Duration::from_millis(60), move || {
        let r = row_idx.get();
        if r < 0 {
            // Done — full sync and unlock
            sync_grid(&s.borrow(), &b.borrow());
            update_score_label(&sl, &s.borrow());
            if s.borrow().has_move() {
                stl.set_label(&format!(
                    "🎉 Level {} — Keep going!",
                    s.borrow().level
                ));
            } else {
                // Even after refill, no moves? Refill again.
                s.borrow_mut().refill_empties();
                sync_grid(&s.borrow(), &b.borrow());
                stl.set_label("🍀 Extra refill — keep going!");
            }
            anim.set(false);
            return glib::ControlFlow::Break;
        }

        let row = r as usize;
        let btns_ref = b.borrow();
        for col in 0..COLS {
            if let Some(k) = game_snapshot.grid[row][col] {
                let btn = &btns_ref[row][col];
                btn.set_label(EMOJI[k]);
                for cls in T_CLS {
                    btn.remove_css_class(cls);
                }
                btn.remove_css_class("empty");
                btn.add_css_class(T_CLS[k]);
                btn.add_css_class("tile-drop");
                // Remove drop class after animation finishes
                let b2 = btn.clone();
                glib::timeout_add_local_once(Duration::from_millis(300), move || {
                    b2.remove_css_class("tile-drop");
                });
            }
        }

        row_idx.set(r - 1);
        glib::ControlFlow::Continue
    });
}

// ── Signal wiring ────────────────────────────────────────────────────────────

fn wire_tile(
    btn: &Button,
    row: usize,
    col: usize,
    state: &Rc<RefCell<Game>>,
    btns: &Rc<RefCell<Vec<Vec<Button>>>>,
    score_lbl: &Label,
    status_lbl: &Label,
    animating: &Rc<Cell<bool>>,
) {
    // 1. Hover — highlight matching group
    let motion = EventControllerMotion::new();
    let s_h = state.clone();
    let b_h = btns.clone();
    let anim_h = animating.clone();
    motion.connect_enter(move |_, _, _| {
        if anim_h.get() {
            return;
        }
        let game = s_h.borrow();
        if game.game_over {
            return;
        }
        let group = game.flood(row, col);
        let valid = group.len() >= MIN_GROUP;
        let btns_ref = b_h.borrow();
        clear_highlights(&btns_ref);
        if valid {
            let set: HashSet<(usize, usize)> = group.into_iter().collect();
            for rr in 0..ROWS {
                for cc in 0..COLS {
                    if set.contains(&(rr, cc)) {
                        btns_ref[rr][cc].add_css_class("tile-glow");
                    } else if game.grid[rr][cc].is_some() {
                        btns_ref[rr][cc].add_css_class("tile-dim");
                    }
                }
            }
        }
    });

    let b_l = btns.clone();
    let anim_l = animating.clone();
    motion.connect_leave(move |_| {
        if !anim_l.get() {
            clear_highlights(&b_l.borrow());
        }
    });
    btn.add_controller(motion);

    // 2. Left Click — clear group with animation
    let s_c = state.clone();
    let b_c = btns.clone();
    let sl_c = score_lbl.clone();
    let stl_c = status_lbl.clone();
    let anim_c = animating.clone();
    btn.connect_clicked(move |_| {
        if anim_c.get() {
            return;
        }
        // Grab the group before mutating
        let group = s_c.borrow().flood(row, col);
        let result = s_c.borrow_mut().click(row, col);
        match result {
            None => {
                if !s_c.borrow().game_over {
                    stl_c.set_label("⚠️  Need 3+ connected matching tiles!");
                }
            }
            Some((n, pts, leveled_up)) => {
                let combo = s_c.borrow().combo;
                clear_highlights(&b_c.borrow());
                animate_removal(
                    &group,
                    &s_c,
                    &b_c,
                    &sl_c,
                    &stl_c,
                    &anim_c,
                    pts,
                    n,
                    combo,
                    leveled_up,
                );
            }
        }
    });

    // 3. Right Click — swap mechanic
    let gesture = GestureClick::new();
    gesture.set_button(3);
    let s_s = state.clone();
    let b_s = btns.clone();
    let sl_s = score_lbl.clone();
    let stl_s = status_lbl.clone();
    let anim_s = animating.clone();

    gesture.connect_pressed(move |_, _, _, _| {
        if anim_s.get() {
            return;
        }
        let mut game = s_s.borrow_mut();
        if game.game_over {
            return;
        }

        if let Some(selected) = game.selected_pos {
            if game.swap_tiles(selected.0, selected.1, row, col) {
                game.selected_pos = None;
                update_score_label(&sl_s, &game);
                stl_s.set_label("🔄 Swapped!");
                sync_grid(&game, &b_s.borrow());
                if game.game_over {
                    let score = game.score;
                    let level = game.level;
                    stl_s.set_label(&format!(
                        "🏁 Game Over! Level {} — Final Score: {}",
                        level, score
                    ));
                }
            } else {
                stl_s.set_label("⚠️ Invalid swap!");
                game.selected_pos = None;
                sync_grid(&game, &b_s.borrow());
            }
        } else if game.grid[row][col].is_some() {
            game.selected_pos = Some((row, col));
            clear_highlights(&b_s.borrow());
            b_s.borrow()[row][col].add_css_class("tile-glow");
            stl_s.set_label("🔀 Right-click an adjacent tile to swap");
        }
    });
    btn.add_controller(gesture);
}

// ── CSS ──────────────────────────────────────────────────────────────────────

fn install_css() {
    let css = CssProvider::new();
    css.load_from_data(STYLE);
    gtk4::style_context_add_provider_for_display(
        &gtk4::gdk::Display::default().unwrap(),
        &css,
        gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
}

const STYLE: &str = r#"
window { background-color: #1a1a2e; }

.game-grid {
    background-color: #0d0d1a;
    padding: 12px;
    border-radius: 12px;
}

.tile {
    border-radius: 8px;
    min-width: 50px;
    min-height: 50px;
    padding: 0;
    transition: background-color 0.2s ease-out,
                opacity 0.2s ease-out;
}

.tile label { font-size: 24px; }

.tile.t0 { background-color: #c0392b; color: white; }
.tile.t1 { background-color: #d35400; color: white; }
.tile.t2 { background-color: #c9a800; color: #222;  }
.tile.t3 { background-color: #7d3c98; color: white; }
.tile.t4 { background-color: #1a5276; color: white; }

.tile:hover {
    background-color: alpha(white, 0.15);
}

.tile.empty {
    background-color: #0a0a16;
    opacity: 0.1;
}

.tile.tile-glow {
    outline: 2px solid rgba(255, 255, 255, 0.8);
    outline-offset: -2px;
    background-image: linear-gradient(rgba(255,255,255,0.15), rgba(255,255,255,0.0));
}

.tile.tile-dim { opacity: 0.25; }

.tile.tile-pop {
    opacity: 0;
    background-color: white;
}

.tile.tile-drop {
    animation: tile-land 0.3s ease-out;
}

@keyframes tile-land {
    0%   { opacity: 0; }
    50%  { opacity: 0.7; }
    100% { opacity: 1; }
}
"#;
